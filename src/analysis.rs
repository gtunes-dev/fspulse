/*
WAV 
WAV64 
DSD 
FLAC 
AIFF 
ALAC (Apple Lossless) 
OGG 
AAC 
MP3 
MQA 
DSF 
and DFF 
*/

/*
Symphonia errors to handle:

pub enum Error {
    IoError(Error),
    DecodeError(&'static str),
    SeekError(SeekErrorKind),
    Unsupported(&'static str),
    LimitError(&'static str),
    ResetRequired,
}

*/

use std::{fmt, fs::File, io::{BufReader, Read}, path::Path};

use hex::encode;
use indicatif::ProgressBar;
use md5::{Digest, Md5};
//use symphonia::core::{codecs::audio::AudioDecoderOptions, errors::Error, formats::{probe::Hint, FormatOptions}, io::MediaSourceStream, meta::MetadataOptions  };
use claxon::{Block, FlacReader};

use crate::error::FsPulseError;


pub struct Analysis {
    // no fields
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValidationState {
    #[default]
    Unknown,
    Valid,
    Invalid,
    NoValidator
}

impl ValidationState {
    pub fn to_str(&self) -> &'static str {
        match self {
            ValidationState::Unknown => "U",
            ValidationState::Valid => "V",
            ValidationState::Invalid => "I",
            ValidationState::NoValidator => "N",
        }
    }

    pub fn from_string(value: &str) -> Self {
        ValidationState::from_char(value.chars().next().unwrap())
    }

    /// Convert a single-character string from the database into a State
    pub fn from_char(value: char) -> Self {
        match value {
            'U' => ValidationState::Unknown,
            'V' => ValidationState::Valid,
            'I' => ValidationState::Invalid,
            'N' => ValidationState::NoValidator,
            _ => ValidationState::Unknown,
        }
    }
}

/// Implement Display to print the short codes directly
impl fmt::Display for ValidationState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Analysis {
    const BLOCKS_PER_TICK: i32 = 500;

    pub fn validate_flac_claxon2(path: &Path, _file_name: &str, is_valid_prog: &ProgressBar) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let mut reader =  match FlacReader::open(path) {
            Ok(reader) => reader,
            Err(e) => {
                let e_str = format!("{:?}", e);
                return Ok((ValidationState::Invalid, Some(e_str)))
            }
        };

        let mut frame_reader = reader.blocks();
        let mut block = Block::empty();

        let mut tick_blocks = 0;

        loop {
            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => block = next_block,
                Ok(None) => break, // EOF.
                Err(error) => {
                    let e_str = format!("{:?}", error);
                        return Ok((ValidationState::Invalid, Some(e_str)))
                },
            }
            tick_blocks += 1;
            if tick_blocks == Analysis::BLOCKS_PER_TICK {
                is_valid_prog.tick();
                tick_blocks = 0;
            }

        }

        Ok((ValidationState::Valid, None))
    }


    pub fn _validate_flac_claxon(path: &Path, _file_name: &str, _is_valid_prog: &ProgressBar) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let mut reader =  match FlacReader::open(path) {
            Ok(reader) => reader,
            Err(e) => {
                let e_str = format!("{:?}", e);
                return Ok((ValidationState::Invalid, Some(e_str)))
            }
        };

        for opt_sample in reader.samples() {
            let _sample = match opt_sample {
                Err(e) => {
                    let e_str = format!("{:?}", e);
                    return Ok((ValidationState::Invalid, Some(e_str)))
                }
                Ok(sample) => sample
            };
        }
        Ok((ValidationState::Valid, None))
    }
    
    /*
    pub fn _validate_flac_symphonia(path: &Path, file_name: &str, is_valid_prog: &ProgressBar) -> Result<bool, FsPulseError> {
            // Try to create a Symphonia decoder
        
        trace!("Begin: Symphonia validate: {}", file_name);
        let f = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(f), Default::default());

        let ext = path.extension();
        let mut hint = Hint::new();
        if let Some(ext_val) = ext {
            hint.with_extension(&ext_val.to_string_lossy());
        }

        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();

        // TOOD: If the file extension matches a known media type that Symphonia supports, we
        // should treat it as a possible validation error if Symphonia can't find a codec

        // Probe the media source.
        //let probed = match symphonia::default::get_probe()
        //    .format(&hint, mss, &fmt_opts, &meta_opts)
        let mut format = match symphonia::default::get_probe()
            .probe(&hint, mss, fmt_opts, meta_opts) {
            Ok(format) => format,
            Err(Error::IoError(io_err)) => {
                trace!("Error::IoError in get_probe: {:?}", io_err);
                is_valid_prog.println(format!("Analysis error ('{}'): {:?}", file_name, io_err));
                if io_err.kind() == io::ErrorKind::UnexpectedEof {  // occurs when file is too short to probe
                    return Ok(true)
                } else {
                    return Ok(false)
                }
            },
            Err(Error::Unsupported(err)) => {
                trace!("Error::Unsupported in get_probe: {:?}", err);
                return Ok(true);
            },
            Err(e) => {
                trace!("Error in get_probe: {:?}", e);
                //error!("{:?}", e);
                return Ok(false)
            }, // Handle all other errors
        };
        
        let dec_opts: AudioDecoderOptions = Default::default();
        //dec_opts.verify = true;

        let tracks = format.tracks().to_vec();
        let track_count = tracks.len();
        
        for track in tracks {
        //for track in tracks {
            
            let track_id = track.id;
            
            let track_codec_params = match track.codec_params.as_ref() {
                Some(code_params) => code_params,
                None => {
                    return Ok(false);
                }
            };
            
            let mut decoder = match symphonia::default::get_codecs().
                make_audio_decoder(track_codec_params.audio().unwrap(), &dec_opts) {

                Ok(decoder) => decoder, // Assign decoder if successful
                Err(symphonia::core::errors::Error::Unsupported(u)) => 
                {
                    trace!("Error::Unsupported in get_codecs: {:?}", u); 
                    return Ok(true) // Handle "Unsupported" error
                },
                Err(e) => {
                    trace!("Error in get_codecs: {:?}", e); 
                    return Ok(false) // Handle all other errors
                },
            };

            let mut track_title: Option<String> = None;

            /* 
            if let Some(metadata) = format.metadata().current() {
                for tag in metadata.tags() {
                    if tag.std == Some(StandardTagKey::TrackTitle) {
                        tag.
                        track_title = Some(tag.());
                        break;
                    }
                }
            }
            */
            
            //let track_title = track_title.unwrap_or_else(|| "unknown".to_string());

            //is_valid_prog.set_message(format!("Validating '{}': Track {} ('{}') of {}", file_name, track_id, track_title, track_count));

            // The decode loop
            loop {
                    // Get the next packet from the media format.
                let packet = match format.next_packet() {
                    Ok(Some(packet)) => packet,
                    Ok(None) => {
                        break;
                    },
                    Err(Error::ResetRequired) => {
                        // The track list has been changed. Re-examine it and create a new set of decoders,
                        // then restart the decode loop. This is an advanced feature and it is not
                        // unreasonable to consider this "the end." As of v0.5.0, the only usage of this is
                        // for chained OGG physical streams.
                        trace!("Error::ResetRequired in next_packet"); 
                        unimplemented!();
                    },
                    Err(Error::IoError(io_err)) => {
                        trace!("Error::IoError in next_packet: {:?}", io_err); 
                        if io_err.kind() == ErrorKind::UnexpectedEof {
                            // This is how Symphonia signals EOF - just break the loop
                            break;
                        } else {
                            return Err(FsPulseError::Error(io_err.to_string()));
                        }
                    },
                    Err(err) => {
                        trace!("Error in next_packet: {:?}", err); 
                        // A unrecoverable error occurred, halt decoding.
                        //println!("{:?}", err);
                        panic!("{}", err);
                    }
                };

                // Consume any new metadata that has been read since the last packet.
                while !format.metadata().is_latest() {
                    // Pop the old head of the metadata queue.
                    format.metadata().pop();

                    // Consume the new metadata at the head of the metadata queue.
                }

                // If the packet does not belong to the selected track, skip over it.
                if packet.track_id() != track.id {
                    continue;
                }

                // Decode the packet into audio samples.
                match decoder.decode(&packet) {
                    Ok(_decoded) => {
                        // Consume the decoded audio samples (see below).
                    }
                    Err(Error::IoError(io_err)) => {
                        // The packet failed to decode due to an IO error, skip the packet.
                        trace!("Error::IoError in decode: {:?}", io_err); 
                        return Ok(false)
                    }
                    Err(Error::DecodeError(err)) => {
                        // The packet failed to decode due to invalid data, skip the packet.
                        trace!("Error::DecodeError in decode: {:?}", err); 
                        return Ok(false)
                        //continue;
                    }
                    Err(err) => {
                        // An unrecoverable error occurred, halt decoding.
                        trace!("Error in decode: {:?}", err); 
                        panic!("{}", err);
                    }
                }
            }
        }
        trace!("End: Symphonia validate: {}", file_name);
        Ok(true)
    }
    */

    pub fn compute_md5_hash(path: &Path, hash_prog: &ProgressBar) -> Result<String, FsPulseError> {
        let f = File::open(path)?;
        let len = f.metadata()?.len();

        // The progress bar is mostly set up by our caller. We just need to set the
        // length and go
        hash_prog.set_length(len);
        
        let mut reader = BufReader::new(f);
        let mut hasher = Md5::new();
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            hash_prog.inc(bytes_read.try_into().unwrap());
        }

        let hash = hasher.finalize();

        Ok(encode(hash))
    }

    pub fn short_md5<'a>(hash: &Option<&'a str>) -> &'a str {
        match hash {
            Some(hash) => &hash[..hash.len().min(7)],
            None =>  "-"
        }
    }
    
}