/// Path sorting utilities for natural, case-insensitive path ordering
///
/// This module provides path comparison functions suitable for use with
/// SQLite collations to achieve human-friendly sorting of file paths.
use std::cmp::Ordering;
use std::path::Path;
use icu_collator::{Collator, CollatorOptions, Strength, Numeric};

/// Create a collator for path segment comparison.
/// Configured with:
/// - Numeric ordering: treats digit sequences as numbers (file2 < file10)
/// - Primary strength: case-insensitive comparison
fn get_collator() -> Collator {
    let mut options = CollatorOptions::new();
    options.strength = Some(Strength::Primary); // Case-insensitive
    options.numeric = Some(Numeric::On); // Natural number ordering

    Collator::try_new(Default::default(), options)
        .expect("Failed to create ICU collator")
}

/// Compare two path strings using natural, case-insensitive ordering.
///
/// This function uses OS-aware path component parsing and compares components
/// segment-by-segment using natural ordering. This ensures proper hierarchical
/// sorting where:
/// - `/proj` comes before `/proj/file` (parent before children)
/// - `/proj/file` comes before `/proj-A` (directory grouping)
/// - Case-insensitive comparison
/// - Natural number ordering (e.g., "file2" < "file10")
/// - Respects OS-specific path separators (\ is a valid filename char on Unix!)
///
/// # Examples
///
/// ```
/// use fspulse::sort::compare_paths;
/// use std::cmp::Ordering;
///
/// assert_eq!(compare_paths("/proj", "/proj-A"), Ordering::Less);
/// assert_eq!(compare_paths("/file2", "/file10"), Ordering::Less);
/// assert_eq!(compare_paths("/a/b", "/a b"), Ordering::Less); // Distinguishes paths from spaces
/// ```
pub fn compare_paths(a: &str, b: &str) -> Ordering {
    let path_a = Path::new(a);
    let path_b = Path::new(b);

    // Use Path::components() to properly parse OS-specific path separators
    // This handles:
    // - Unix: / as separator, \ is valid in filenames
    // - Windows: \ as separator, / may also work
    // - Handles . and .. correctly
    let components_a: Vec<_> = path_a.components().collect();
    let components_b: Vec<_> = path_b.components().collect();

    // Create collator once for this comparison
    let collator = get_collator();

    // Compare component by component using ICU collator
    for (comp_a, comp_b) in components_a.iter().zip(components_b.iter()) {
        // Convert components to string slices for comparison
        let seg_a = comp_a.as_os_str().to_string_lossy();
        let seg_b = comp_b.as_os_str().to_string_lossy();

        let cmp = collator.compare(&seg_a, &seg_b);
        if cmp != Ordering::Equal {
            return cmp;
        }
    }

    // If all components match so far, shorter path comes first
    // This ensures `/proj` comes before `/proj/file`
    components_a.len().cmp(&components_b.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to sort a list of paths and return the sorted order
    fn sort_paths(mut paths: Vec<&str>) -> Vec<&str> {
        paths.sort_by(|a, b| compare_paths(a, b));
        paths
    }

    #[test]
    fn test_user_example_delimiter_issue() {
        // The original issue: /proj and its files should appear before /proj-A
        // because '-' (ASCII 45) sorts before '/' (ASCII 47) in binary comparison
        let paths = vec![
            "/proj-A/file1",
            "/proj",
            "/proj/file3",
            "/proj/file2",
        ];

        let sorted = sort_paths(paths);

        // Expected: /proj comes first, then its children, then /proj-A
        assert_eq!(
            sorted,
            vec![
                "/proj",
                "/proj/file2",
                "/proj/file3",
                "/proj-A/file1",
            ],
            "Paths should group /proj with its children before /proj-A"
        );
    }

    #[test]
    fn test_space_vs_slash_distinction() {
        // Critical test: paths with slashes should not be confused with paths containing spaces
        let paths = vec![
            "/a b",     // Path with space in name
            "/a/b",     // Path with subdirectory
            "/a",       // Parent directory
        ];

        let sorted = sort_paths(paths);

        // The slash creates a hierarchy, space is just part of the name
        // Expected order: /a, /a/b (subdirectory), /a b (different name)
        assert_eq!(
            sorted,
            vec![
                "/a",
                "/a/b",     // Subdirectory of /a
                "/a b",     // Different name entirely (space sorts after slash segment)
            ],
            "Paths with slashes must be distinguished from paths with spaces"
        );
    }

    #[test]
    fn test_case_insensitivity() {
        let paths = vec![
            "/Users/Alice/file",
            "/users/bob/file",
            "/USERS/charlie/file",
        ];

        let sorted = sort_paths(paths.clone());

        // First segment "Users"/"users"/"USERS" is case-insensitive equal
        // So should sort by second segment: Alice < bob < charlie (case-insensitive)
        assert_eq!(
            sorted,
            vec![
                "/Users/Alice/file",    // Alice sorts first
                "/users/bob/file",      // bob sorts second
                "/USERS/charlie/file",  // charlie sorts third
            ],
            "Should sort by second segment (Alice < bob < charlie) when first segments are equal case-insensitively"
        );

        // Verify the sort is truly case-insensitive
        assert_eq!(compare_paths("/users/a", "/Users/a"), Ordering::Equal);
        assert_eq!(compare_paths("/users/a", "/USERS/b"), Ordering::Less);
    }

    #[test]
    fn test_natural_number_ordering() {
        let paths = vec![
            "/file10.txt",
            "/file2.txt",
            "/file1.txt",
            "/file100.txt",
        ];

        let sorted = sort_paths(paths);

        // Natural ordering: 1, 2, 10, 100 (not lexicographic: 1, 10, 100, 2)
        assert_eq!(
            sorted,
            vec![
                "/file1.txt",
                "/file2.txt",
                "/file10.txt",
                "/file100.txt",
            ]
        );
    }

    #[test]
    fn test_mixed_case_and_numbers() {
        let paths = vec![
            "/Project10/FileB",
            "/project2/FileA",
            "/PROJECT1/fileC",
        ];

        let sorted = sort_paths(paths);

        // Should handle both case-insensitivity and natural number ordering
        assert_eq!(
            sorted,
            vec![
                "/PROJECT1/fileC",
                "/project2/FileA",
                "/Project10/FileB",
            ]
        );
    }

    #[test]
    fn test_directory_hierarchy() {
        let paths = vec![
            "/a/b/c/file",
            "/a/file",
            "/a/b/file",
            "/a",
        ];

        let sorted = sort_paths(paths);

        // Parent directories should come before their contents
        // or at least be sorted consistently
        assert_eq!(
            sorted,
            vec![
                "/a",
                "/a/b/c/file",
                "/a/b/file",
                "/a/file",
            ]
        );
    }

    #[test]
    fn test_special_characters() {
        let paths = vec![
            "/file-name",
            "/file_name",
            "/file.name",
            "/file name",
        ];

        let sorted = sort_paths(paths);

        // Just verify it doesn't panic and produces consistent ordering
        // The exact order depends on lexical-sort's rules
        assert_eq!(sorted.len(), 4);
    }

    #[test]
    fn test_path_components_handling() {
        // Test that Path::components() correctly handles OS-specific separators
        // This test works on all platforms because we test with forward slashes,
        // which work everywhere

        let paths = vec![
            "/test/subfolder/file",
            "/test/file",
            "/test",
            "/test2/file",
        ];

        let sorted = sort_paths(paths);

        // Verify hierarchical sorting
        assert_eq!(sorted[0], "/test");
        assert_eq!(sorted[1], "/test/file");
        assert_eq!(sorted[2], "/test/subfolder/file");
        assert_eq!(sorted[3], "/test2/file");

        // The key point: Path::components() handles platform-specific separators
        // - On Unix/Mac: / is the separator
        // - On Windows: both \ and / work as separators
        // - Our code doesn't need to know which platform it's on
    }

    #[test]
    fn test_backslash_behavior() {
        // Test behavior with backslashes in paths
        // Behavior differs by platform, but Path::components() handles it correctly

        let path_with_backslash = "/test\\file";
        let path_with_slash = "/test/file";

        let result = compare_paths(path_with_backslash, path_with_slash);

        // On Windows: both are equivalent (both are separators)
        // On Unix: different (backslash is part of filename)
        // We don't assert the specific result, just that it doesn't panic
        // and produces a valid Ordering
        assert!(result == Ordering::Less || result == Ordering::Greater || result == Ordering::Equal);

        // The important thing is that Path::components() handles this correctly
        // for the platform it's running on, and our code doesn't need special cases
    }

    #[test]
    fn test_unicode_paths() {
        let paths = vec![
            "/café",
            "/resume",
            "/résumé",
        ];

        let sorted = sort_paths(paths);

        // lexical-sort should handle unicode normalization
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn test_empty_and_edge_cases() {
        assert_eq!(compare_paths("", ""), Ordering::Equal);
        assert_eq!(compare_paths("", "/file"), Ordering::Less);
        assert_eq!(compare_paths("/file", ""), Ordering::Greater);
        assert_eq!(compare_paths("/", "/"), Ordering::Equal);
    }

    #[test]
    fn test_real_world_example() {
        // Simulate a real directory listing
        let paths = vec![
            "/home/user/Documents/report2023.pdf",
            "/home/user/Documents/report2024.pdf",
            "/home/user/Documents/Report10.pdf",
            "/home/user/documents/archive",  // lowercase 'documents'
            "/home/user/Documents",
        ];

        let sorted = sort_paths(paths);

        // Verify Documents comes first, then its contents
        // Numbers sorted naturally, case-insensitive
        assert_eq!(sorted[0], "/home/user/Documents");
        assert!(sorted.contains(&"/home/user/Documents/Report10.pdf"));
        assert!(sorted.contains(&"/home/user/Documents/report2023.pdf"));
    }

    #[test]
    fn test_icu_collator_numeric_overflow() {
        // Test if icu_collator handles large numbers gracefully (no panic)
        // This was a problem with the previous lexical-sort library

        println!("Testing icu_collator with large numbers...");

        let paths = vec![
            "/file00000000000000000000",
            "/file18446744073709551616",
            "/file99999999999999999999999999999999",
        ];

        let result = std::panic::catch_unwind(|| {
            sort_paths(paths)
        });

        match result {
            Ok(sorted) => {
                println!("✓ No panic! icu_collator handles large numbers safely");
                println!("  Sorted: {:?}", sorted);
                assert_eq!(sorted.len(), 3);
            }
            Err(_) => {
                panic!("icu_collator panicked on large numbers");
            }
        }
    }

    #[test]
    fn test_icu_collator_without_splitting() {
        // Test if icu_collator alone handles path delimiters correctly
        // WITHOUT our segment-by-segment splitting approach

        let collator = get_collator();

        // The critical test: does icu_collator understand that '/' should
        // group paths correctly, or do we need to split?
        let cmp1 = collator.compare("/proj", "/proj-A/file1");
        let cmp2 = collator.compare("/proj/file2", "/proj-A/file1");

        println!("Without splitting:");
        println!("  '/proj' vs '/proj-A/file1': {:?}", cmp1);
        println!("  '/proj/file2' vs '/proj-A/file1': {:?}", cmp2);

        // If icu_collator handles delimiters correctly:
        // - /proj should come before /proj-A (cmp1 should be Less)
        // - /proj/file2 should come before /proj-A (cmp2 should be Less)

        if cmp1 == Ordering::Less && cmp2 == Ordering::Less {
            println!("✓ icu_collator handles path delimiters correctly!");
            println!("  We may not need segment splitting");
        } else {
            println!("✗ icu_collator does NOT handle path delimiters specially");
            println!("  We NEED segment splitting to get correct ordering");
            println!("  Expected: /proj < /proj-A and /proj/file2 < /proj-A");
            println!("  Got: cmp1={:?}, cmp2={:?}", cmp1, cmp2);
        }

        // This test documents the behavior but doesn't assert
        // We'll decide based on the output what approach to take
    }

    #[test]
    fn test_delimiter_sorting_comparison() {
        // Compare our segment-based approach vs direct icu_collator comparison

        let collator = get_collator();
        let paths = vec![
            "/proj-A/file1",
            "/proj",
            "/proj/file3",
            "/proj/file2",
        ];

        // Test 1: Direct icu_collator comparison (no splitting)
        let mut paths_direct = paths.clone();
        paths_direct.sort_by(|a, b| collator.compare(a, b));

        println!("Direct icu_collator (no splitting): {:?}", paths_direct);

        // Test 2: Our segment-based approach
        let paths_segmented = sort_paths(paths.clone());

        println!("Segment-based approach: {:?}", paths_segmented);

        // Expected correct order
        let expected = vec![
            "/proj",
            "/proj/file2",
            "/proj/file3",
            "/proj-A/file1",
        ];

        println!("Expected order: {:?}", expected);

        // Check which approach gives correct results
        if paths_direct == expected {
            println!("✓ Direct icu_collator gives correct order - splitting may be unnecessary!");
        } else {
            println!("✗ Direct icu_collator gives wrong order - splitting is required");
        }

        if paths_segmented == expected {
            println!("✓ Segment-based approach gives correct order");
        } else {
            println!("✗ Segment-based approach gives wrong order");
        }

        // Assert that our current approach works correctly
        assert_eq!(paths_segmented, expected,
            "Segment-based approach must produce correct ordering");
    }
}
