use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

use parakeet_cpp_sys as sys;

use crate::error::Error;
use crate::options::{TranscribeOptions, Transcript};

/// The ABI this binding was written against. Bump together with the upstream pin.
pub const SUPPORTED_ABI: i32 = 4;

pub struct Model {
    pub(crate) ctx: *mut sys::parakeet_ctx,
    streaming: bool,
}

// SAFETY: all mutable state is encapsulated in `ctx`; the C library is assumed
// to keep no thread-local state tied to the calling thread (no errno-style
// per-thread error buffers, no thread-affine handles). `transcribe` takes
// `&mut self`, so calls are exclusive; moving ownership across threads is sound.
// `*mut parakeet_ctx` keeps the type `!Sync`, preventing shared concurrent use.
unsafe impl Send for Model {}

impl Model {
    pub fn load(gguf_path: &Path) -> Result<Self, Error> {
        let abi = unsafe { sys::parakeet_capi_abi_version() };
        if abi != SUPPORTED_ABI {
            return Err(Error::AbiMismatch {
                expected: SUPPORTED_ABI,
                found: abi,
            });
        }
        let c_path = CString::new(gguf_path.to_string_lossy().as_bytes())
            .map_err(|_| Error::Load("path contains NUL".into()))?;
        let ctx = unsafe { sys::parakeet_capi_load(c_path.as_ptr()) };
        if ctx.is_null() {
            // parakeet_capi_last_error requires a live ctx; on load failure ctx is NULL
            // and the C API exposes no global error slot, so we can only report the path.
            return Err(Error::Load(format!(
                "parakeet_capi_load returned NULL for {}",
                gguf_path.display()
            )));
        }
        // Probe streaming support: stream_begin returns NULL for offline models.
        // Assumes begin+immediate-free leaves the ctx unaffected for later transcribe.
        let probe = unsafe { sys::parakeet_capi_stream_begin(ctx) };
        let streaming = !probe.is_null();
        if !probe.is_null() {
            unsafe { sys::parakeet_capi_stream_free(probe) };
        }
        Ok(Self { ctx, streaming })
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// Offline one-shot transcription of 16 kHz mono f32 PCM.
    pub fn transcribe(
        &mut self,
        pcm: &[f32],
        sample_rate: u32,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, Error> {
        let n =
            i32::try_from(pcm.len()).map_err(|_| Error::Transcribe("too many samples".into()))?;
        let decoder = 0; // default (by arch)
        #[allow(clippy::cast_possible_wrap)] // sample rates are always < 2^31
        let sr = sample_rate as i32;
        let raw = match &opts.language {
            Some(lang) => {
                let c_lang = CString::new(lang.as_str())
                    .map_err(|_| Error::Transcribe("lang has NUL".into()))?;
                unsafe {
                    sys::parakeet_capi_transcribe_pcm_lang(
                        self.ctx,
                        pcm.as_ptr(),
                        n,
                        sr,
                        decoder,
                        c_lang.as_ptr(),
                    )
                }
            }
            None => unsafe {
                sys::parakeet_capi_transcribe_pcm(self.ctx, pcm.as_ptr(), n, sr, decoder)
            },
        };
        let text = self.take_string(raw)?;
        Ok(Transcript {
            text,
            words: Vec::new(),
        })
    }

    /// Convert a malloc'd C string into an owned Rust String and free it.
    /// NULL means failure — surface the ctx's last_error.
    pub(crate) fn take_string(&self, raw: *mut std::os::raw::c_char) -> Result<String, Error> {
        if raw.is_null() {
            return Err(Error::Transcribe(self.last_error()));
        }
        // SAFETY: `raw` is a malloc'd NUL-terminated UTF-8 string returned by
        // parakeet.cpp and owned by us. We fully materialize an owned String (or
        // detect the UTF-8 error) BEFORE calling free_string, so no borrow outlives
        // the allocation. free_string runs on every path (success and Utf8 error),
        // exactly once; `raw` is never touched again afterward.
        let s = unsafe { CStr::from_ptr(raw) }
            .to_str()
            .map(|s| s.to_owned());
        unsafe { sys::parakeet_capi_free_string(raw) };
        s.map_err(|_| Error::Utf8)
    }

    pub(crate) fn last_error(&self) -> String {
        let p = unsafe { sys::parakeet_capi_last_error(self.ctx) };
        if p.is_null() {
            return "unknown error".into();
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            unsafe { sys::parakeet_capi_free(self.ctx) };
            self.ctx = ptr::null_mut();
        }
    }
}
