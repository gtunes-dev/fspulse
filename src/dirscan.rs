use crate::error::DirCheckError;
use crate::database::{ Database, ItemType, ChangeType };

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Default)]
struct ChangeCounts {
    add_count: u32,
    modify_count: u32,
    delete_count: u32,
    type_change_count: u32,
    unchanged_count: u32,
}

pub struct DirScan<'a> {
    change_counts: ChangeCounts,
    db: &'a mut Database,
    user_path: &'a Path,
    absolute_path: PathBuf,
}

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}

impl<'a> DirScan<'a> {
    fn new(db: &'a mut Database, user_path: &'a Path, absolute_path: PathBuf) -> Self {
        Self {
            change_counts: ChangeCounts::default(),
            db,
            user_path,
            absolute_path,
        }
    }

    pub fn scan_directory(db: &mut Database, user_path: &Path) -> Result<(), DirCheckError> {
        let absolute_path = DirScan::validate_and_resolve_path(user_path)?;

        let mut dir_scan = DirScan::new(db, user_path, absolute_path);
        dir_scan.do_scan_directory()?;
        dir_scan.print_scan_results();

        Ok(())
    }

    fn do_scan_directory(&mut self) -> Result<(), DirCheckError> {    
        let metadata = fs::symlink_metadata(&self.absolute_path)?;
    
        self.db.begin_scan(&self.absolute_path.to_string_lossy())?;
    
        let mut q = VecDeque::new();
    
        q.push_back(QueueEntry {
            path: self.absolute_path.clone(),
            metadata,
        });
    
        while let Some(q_entry) = q.pop_front() {
            // println!("Directory: {}", q_entry.path.display());
    
            // Update the database
            let change_type = self.db.handle_item(ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;
            self.update_change_counts(change_type);
    
            let entries = fs::read_dir(&q_entry.path)?;
    
            for entry in entries {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?; // Use symlink_metadata to check for symlinks
    
                if metadata.is_dir() {
                    q.push_back(QueueEntry {
                        path: entry.path(),
                        metadata,
                    });
                } else {
                    let item_type = if metadata.is_file() {
                        ItemType::File
                    } else if metadata.is_symlink() {
                        ItemType::Symlink
                    } else {
                        ItemType::Other
                    };
    
                    // println!("{:?}: {}", item_type, entry.path().display());
                    
                    // Update the database
                    self.db.handle_item(item_type, &entry.path(), &metadata)?;
                }
            }
        }
        self.db.end_scan()?;
    
        Ok(())
    }
    
    fn validate_and_resolve_path(user_path: &Path) -> Result<PathBuf, DirCheckError> {
        if user_path.as_os_str().is_empty() {
            return Err(DirCheckError::Error("Provided path is empty".to_string()));
        }
    
        let absolute_path = if user_path.is_absolute() {
            user_path.to_owned()
        }  else {
            env::current_dir()?.join(user_path)
        };
        
        if !absolute_path.exists() {
            return Err(DirCheckError::Error(format!("Path '{}' does not exist", absolute_path.display())));
        }
    
        let metadata = fs::symlink_metadata(&absolute_path)?;
        if metadata.file_type().is_symlink() {
            return Err(DirCheckError::Error(format!("Path '{}' is a symlink and not allowed", absolute_path.display())));
        }
        
        if !metadata.is_dir() {
            return Err(DirCheckError::Error(format!("Path '{}' is not a directory", absolute_path.display())));
        }
    
        Ok(absolute_path)
    }

    fn update_change_counts(&mut self, change_type: ChangeType) {
        match change_type {
            ChangeType::Add => self.change_counts.add_count += 1,
            ChangeType::Modify => self.change_counts.modify_count += 1,
            ChangeType::Delete => self.change_counts.delete_count += 1,
            ChangeType::TypeChange => self.change_counts.type_change_count += 1,
            ChangeType::NoChange => self.change_counts.unchanged_count += 1,
        }
    }

    fn print_scan_results(&self) {
        println!("Scan Results: {}", self.absolute_path.display());
        println!("-------------");
        println!("{:<12} {}", "Added:", self.change_counts.add_count);
        println!("{:<12} {}", "Modified:", self.change_counts.modify_count);
        println!("{:<12} {}", "Deleted:", self.change_counts.delete_count);
        println!("{:<12} {}", "Type Change:", self.change_counts.type_change_count);
        println!("{:<12} {}", "No Change:", self.change_counts.unchanged_count);
        println!();
    }
}