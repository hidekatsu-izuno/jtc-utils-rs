//! Streaming transcoding adapters for standard `Read`/`Write` consumers.
//! A writer must be finalized with [`EncodeWriter::finish`] to validate trailing
//! UTF-8 and emit final shift sequences. `flush` alone does not end the stream.

use crate::charset::{Charset, DecodeOptions, Decoder, EncodeOptions, Encoder};
use std::io::{self, Read, Write};

/// A reader exposing encoded input as UTF-8 bytes.
pub struct DecodeReader<R> {
    inner: R,
    decoder: Decoder,
    buffer: Vec<u8>,
    position: usize,
    done: bool,
}

impl<R: Read> DecodeReader<R> {
    /// Wrap a reader with strict decoding and matching BOM removal.
    pub fn new(inner: R, charset: Charset) -> Self {
        Self::with_options(inner, charset, DecodeOptions::default())
    }

    /// Wrap a reader with explicit decoding options.
    pub fn with_options(inner: R, charset: Charset, options: DecodeOptions) -> Self {
        Self {
            inner,
            decoder: charset.decoder_with_options(options),
            buffer: Vec::new(),
            position: 0,
            done: false,
        }
    }

    /// Borrow the underlying input.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Recover the underlying input. Any buffered decoded data is discarded.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for DecodeReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.position == self.buffer.len() {
            if self.done {
                return Ok(0);
            }
            let mut input = [0; 8192];
            let size = match self.inner.read(&mut input) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            self.done = size == 0;
            match self.decoder.decode(&input[..size], self.done) {
                Ok(text) => self.buffer = text.into_bytes(),
                Err(error) => {
                    self.done = true;
                    return Err(io::Error::new(io::ErrorKind::InvalidData, error));
                }
            }
            self.position = 0;
        }
        let size = output.len().min(self.buffer.len() - self.position);
        output[..size].copy_from_slice(&self.buffer[self.position..self.position + size]);
        self.position += size;
        Ok(size)
    }
}

/// A writer accepting UTF-8 bytes and writing the selected encoding.
/// UTF-8 scalars may be split across calls to `write`.
pub struct EncodeWriter<W: Write> {
    inner: W,
    encoder: Encoder,
    pending: Vec<u8>,
    failed: bool,
}

impl<W: Write> EncodeWriter<W> {
    /// Wrap a writer using strict conversion.
    pub fn new(inner: W, charset: Charset) -> Self {
        Self::with_options(inner, charset, EncodeOptions::default())
    }

    /// Wrap a writer with explicit encoding options.
    pub fn with_options(inner: W, charset: Charset, options: EncodeOptions) -> Self {
        Self {
            inner,
            encoder: charset.encoder_with_options(options),
            pending: Vec::new(),
            failed: false,
        }
    }

    /// Borrow the destination.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Validate pending UTF-8, finalize the charset, flush, and return the output.
    /// Dropping a writer does not finalize it or report conversion errors.
    pub fn finish(mut self) -> io::Result<W> {
        self.check()?;
        if !self.pending.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incomplete UTF-8 at end of stream",
            ));
        }
        let bytes = self
            .encoder
            .finish()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.inner.write_all(&bytes)?;
        self.inner.flush()?;
        Ok(self.inner)
    }

    fn check(&self) -> io::Result<()> {
        if self.failed {
            Err(io::Error::other("transcoding writer failed previously"))
        } else {
            Ok(())
        }
    }

    fn write_chunk(&mut self, input: &[u8]) -> io::Result<()> {
        let mut bytes = std::mem::take(&mut self.pending);
        bytes.extend_from_slice(input);
        let valid = match std::str::from_utf8(&bytes) {
            Ok(text) => text.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        let text = std::str::from_utf8(&bytes[..valid]).unwrap();
        let encoded = self
            .encoder
            .encode(text, false)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.inner.write_all(&encoded)?;
        self.pending.extend_from_slice(&bytes[valid..]);
        Ok(())
    }
}

impl<W: Write> Write for EncodeWriter<W> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.check()?;
        if input.is_empty() {
            return Ok(0);
        }
        let result = self.write_chunk(input);
        if result.is_err() {
            self.failed = true;
        }
        result.map(|()| input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.check()?;
        let result = self.inner.flush();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
