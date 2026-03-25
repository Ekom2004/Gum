use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::c_int;
use std::ptr;
use std::sync::Once;

mod ffi {
    #![allow(non_camel_case_types)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(non_upper_case_globals)]
    #![allow(unnecessary_transmutes)]
    #![allow(unused)]
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

static NETWORK_INIT: Once = Once::new();
const AVERROR_EOF_CODE: c_int = fferrtag(b'E', b'O', b'F', b' ');
const AV_NOPTS_VALUE_I64: i64 = i64::MIN;
const SWS_BILINEAR_FLAG: c_int = 1 << 1;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractFramesRequest {
    pub source_uri: String,
    pub scan_start_ms: u64,
    pub scan_end_ms: u64,
    pub sample_fps: f32,
    pub frame_width: u32,
    pub frame_height: u32,
}

impl ExtractFramesRequest {
    pub fn validate(&self) -> Result<(), ExtractFramesError> {
        if self.source_uri.trim().is_empty() {
            return Err(ExtractFramesError::InvalidRequest(
                "source_uri must be non-empty".to_string(),
            ));
        }
        if self.scan_end_ms <= self.scan_start_ms {
            return Err(ExtractFramesError::InvalidRequest(
                "scan_end_ms must be greater than scan_start_ms".to_string(),
            ));
        }
        if !self.sample_fps.is_finite() || self.sample_fps <= 0.0 {
            return Err(ExtractFramesError::InvalidRequest(
                "sample_fps must be finite and greater than zero".to_string(),
            ));
        }
        if self.frame_width == 0 || self.frame_height == 0 {
            return Err(ExtractFramesError::InvalidRequest(
                "frame dimensions must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedRgbFrame {
    pub timestamp_ms: u64,
    pub width: u32,
    pub height: u32,
    pub rgb24: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractFramesResponse {
    pub frames: Vec<ExtractedRgbFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FfmpegVersionTriplet {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl FfmpegVersionTriplet {
    pub fn from_packed(version: u32) -> Self {
        Self {
            major: version >> 16,
            minor: (version >> 8) & 0xff,
            patch: version & 0xff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkedFfmpegVersions {
    pub avformat: FfmpegVersionTriplet,
    pub avcodec: FfmpegVersionTriplet,
    pub avutil: FfmpegVersionTriplet,
    pub swscale: FfmpegVersionTriplet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractFramesError {
    InvalidRequest(String),
    Ffmpeg(String),
}

impl fmt::Display for ExtractFramesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => f.write_str(message),
            Self::Ffmpeg(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ExtractFramesError {}

pub fn extract_sampled_rgb_frames(
    request: &ExtractFramesRequest,
) -> Result<ExtractFramesResponse, ExtractFramesError> {
    request.validate()?;
    init_network();

    let source = CString::new(request.source_uri.as_str()).map_err(|_| {
        ExtractFramesError::InvalidRequest("source_uri contains embedded NUL byte".to_string())
    })?;

    unsafe {
        let mut fmt_ptr: *mut ffi::AVFormatContext = ptr::null_mut();
        ffmpeg_call(
            ffi::avformat_open_input(
                &mut fmt_ptr,
                source.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            ),
            "avformat_open_input",
        )?;
        let fmt = FormatContext(fmt_ptr);

        ffmpeg_call(
            ffi::avformat_find_stream_info(fmt.0, ptr::null_mut()),
            "avformat_find_stream_info",
        )?;

        let stream_index = ffmpeg_call(
            ffi::av_find_best_stream(
                fmt.0,
                ffi::AVMediaType_AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                ptr::null_mut(),
                0,
            ),
            "av_find_best_stream",
        )?;

        let stream = *(*fmt.0).streams.add(stream_index as usize);
        let codecpar = (*stream).codecpar;
        if codecpar.is_null() {
            return Err(ExtractFramesError::Ffmpeg(
                "video stream codec parameters missing".to_string(),
            ));
        }

        let codec = ffi::avcodec_find_decoder((*codecpar).codec_id);
        if codec.is_null() {
            return Err(ExtractFramesError::Ffmpeg(
                "no decoder found for video stream".to_string(),
            ));
        }

        let codec_ptr = ffi::avcodec_alloc_context3(codec);
        if codec_ptr.is_null() {
            return Err(ExtractFramesError::Ffmpeg(
                "avcodec_alloc_context3 returned null".to_string(),
            ));
        }
        let codec_ctx = CodecContext(codec_ptr);

        ffmpeg_call(
            ffi::avcodec_parameters_to_context(codec_ctx.0, codecpar),
            "avcodec_parameters_to_context",
        )?;
        ffmpeg_call(
            ffi::avcodec_open2(codec_ctx.0, codec, ptr::null_mut()),
            "avcodec_open2",
        )?;

        let seek_ts = ffi::av_rescale_q(
            request.scan_start_ms as i64,
            ms_time_base(),
            (*stream).time_base,
        );
        ffmpeg_call(
            ffi::av_seek_frame(
                fmt.0,
                stream_index,
                seek_ts,
                ffi::AVSEEK_FLAG_BACKWARD as c_int,
            ),
            "av_seek_frame",
        )?;
        ffi::avcodec_flush_buffers(codec_ctx.0);

        let packet = Packet::new()?;
        let frame = Frame::new()?;
        let mut scaler = Scaler(ptr::null_mut());
        let mut next_keep_ms = request.scan_start_ms as i64;
        let step_ms = frame_step_ms(request.sample_fps);
        let mut frames = Vec::new();

        loop {
            let read_ret = ffi::av_read_frame(fmt.0, packet.0);
            if read_ret < 0 {
                break;
            }
            if (*packet.0).stream_index != stream_index {
                ffi::av_packet_unref(packet.0);
                continue;
            }

            let send_ret = ffi::avcodec_send_packet(codec_ctx.0, packet.0);
            ffi::av_packet_unref(packet.0);
            if send_ret < 0 && send_ret != again_error_code() {
                return Err(ffmpeg_error(send_ret, "avcodec_send_packet"));
            }

            let reached_end = receive_available_frames(
                codec_ctx.0,
                stream,
                frame.0,
                &mut scaler,
                request,
                &mut next_keep_ms,
                step_ms,
                &mut frames,
            )?;
            if reached_end {
                break;
            }
        }

        let drain_ret = ffi::avcodec_send_packet(codec_ctx.0, ptr::null());
        if drain_ret < 0 && drain_ret != again_error_code() && drain_ret != AVERROR_EOF_CODE {
            return Err(ffmpeg_error(drain_ret, "avcodec_send_packet(drain)"));
        }
        let _ = receive_available_frames(
            codec_ctx.0,
            stream,
            frame.0,
            &mut scaler,
            request,
            &mut next_keep_ms,
            step_ms,
            &mut frames,
        )?;

        Ok(ExtractFramesResponse { frames })
    }
}

pub fn linked_ffmpeg_versions() -> LinkedFfmpegVersions {
    unsafe {
        LinkedFfmpegVersions {
            avformat: FfmpegVersionTriplet::from_packed(ffi::avformat_version()),
            avcodec: FfmpegVersionTriplet::from_packed(ffi::avcodec_version()),
            avutil: FfmpegVersionTriplet::from_packed(ffi::avutil_version()),
            swscale: FfmpegVersionTriplet::from_packed(ffi::swscale_version()),
        }
    }
}

unsafe fn receive_available_frames(
    codec_ctx: *mut ffi::AVCodecContext,
    stream: *mut ffi::AVStream,
    frame: *mut ffi::AVFrame,
    scaler: &mut Scaler,
    request: &ExtractFramesRequest,
    next_keep_ms: &mut i64,
    step_ms: i64,
    frames: &mut Vec<ExtractedRgbFrame>,
) -> Result<bool, ExtractFramesError> {
    loop {
        let recv_ret = ffi::avcodec_receive_frame(codec_ctx, frame);
        if recv_ret == again_error_code() || recv_ret == AVERROR_EOF_CODE {
            return Ok(false);
        }
        if recv_ret < 0 {
            return Err(ffmpeg_error(recv_ret, "avcodec_receive_frame"));
        }

        let timestamp_ms = frame_timestamp_ms(stream, frame)?;
        if timestamp_ms >= request.scan_end_ms as i64 {
            ffi::av_frame_unref(frame);
            return Ok(true);
        }
        if timestamp_ms < request.scan_start_ms as i64 || timestamp_ms < *next_keep_ms {
            ffi::av_frame_unref(frame);
            continue;
        }

        let rgb24 = scale_and_pad_frame(frame, scaler, request)?;
        frames.push(ExtractedRgbFrame {
            timestamp_ms: timestamp_ms as u64,
            width: request.frame_width,
            height: request.frame_height,
            rgb24,
        });
        while *next_keep_ms <= timestamp_ms {
            *next_keep_ms += step_ms;
        }
        ffi::av_frame_unref(frame);
    }
}

unsafe fn frame_timestamp_ms(
    stream: *mut ffi::AVStream,
    frame: *mut ffi::AVFrame,
) -> Result<i64, ExtractFramesError> {
    let pts = if (*frame).best_effort_timestamp != AV_NOPTS_VALUE_I64 {
        (*frame).best_effort_timestamp
    } else if (*frame).pts != AV_NOPTS_VALUE_I64 {
        (*frame).pts
    } else {
        return Err(ExtractFramesError::Ffmpeg(
            "decoded frame missing timestamp".to_string(),
        ));
    };
    Ok(ffi::av_rescale_q(pts, (*stream).time_base, ms_time_base()))
}

unsafe fn scale_and_pad_frame(
    frame: *mut ffi::AVFrame,
    scaler: &mut Scaler,
    request: &ExtractFramesRequest,
) -> Result<Vec<u8>, ExtractFramesError> {
    let src_width = (*frame).width.max(1) as u32;
    let src_height = (*frame).height.max(1) as u32;
    let src_format = (*frame).format;
    let (scaled_width, scaled_height) = fit_inside(
        src_width,
        src_height,
        request.frame_width,
        request.frame_height,
    );

    scaler.ensure(
        src_width as c_int,
        src_height as c_int,
        src_format,
        scaled_width as c_int,
        scaled_height as c_int,
    )?;

    let mut scaled_rgb = vec![0_u8; (scaled_width * scaled_height * 3) as usize];
    let mut dst_data = [ptr::null_mut(); 4];
    let mut dst_linesize = [0_i32; 4];
    dst_data[0] = scaled_rgb.as_mut_ptr();
    dst_linesize[0] = (scaled_width * 3) as c_int;

    let scale_ret = ffi::sws_scale(
        scaler.0,
        (*frame).data.as_ptr() as *const *const u8,
        (*frame).linesize.as_ptr(),
        0,
        (*frame).height,
        dst_data.as_mut_ptr(),
        dst_linesize.as_mut_ptr(),
    );
    if scale_ret < 0 {
        return Err(ffmpeg_error(scale_ret, "sws_scale"));
    }

    let mut final_rgb = vec![0_u8; (request.frame_width * request.frame_height * 3) as usize];
    let pad_x = ((request.frame_width - scaled_width) / 2) as usize;
    let pad_y = ((request.frame_height - scaled_height) / 2) as usize;
    let target_stride = (request.frame_width * 3) as usize;
    let scaled_stride = (scaled_width * 3) as usize;
    for row in 0..scaled_height as usize {
        let src_offset = row * scaled_stride;
        let dst_offset = (row + pad_y) * target_stride + (pad_x * 3);
        final_rgb[dst_offset..dst_offset + scaled_stride]
            .copy_from_slice(&scaled_rgb[src_offset..src_offset + scaled_stride]);
    }

    Ok(final_rgb)
}

fn fit_inside(src_width: u32, src_height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = f64::min(
        max_width as f64 / src_width as f64,
        max_height as f64 / src_height as f64,
    );
    let width = ((src_width as f64 * scale).round() as u32).clamp(1, max_width);
    let height = ((src_height as f64 * scale).round() as u32).clamp(1, max_height);
    (width, height)
}

fn frame_step_ms(sample_fps: f32) -> i64 {
    ((1000.0 / sample_fps.max(0.001)).round() as i64).max(1)
}

fn ms_time_base() -> ffi::AVRational {
    ffi::AVRational { num: 1, den: 1000 }
}

fn ffmpeg_call(ret: c_int, op: &str) -> Result<c_int, ExtractFramesError> {
    if ret < 0 {
        Err(ffmpeg_error(ret, op))
    } else {
        Ok(ret)
    }
}

fn ffmpeg_error(ret: c_int, op: &str) -> ExtractFramesError {
    let mut buffer = [0_i8; 256];
    unsafe {
        let _ = ffi::av_strerror(ret, buffer.as_mut_ptr(), buffer.len());
        let message = CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .trim()
            .to_string();
        ExtractFramesError::Ffmpeg(format!("{op} failed: {message} ({ret})"))
    }
}

fn again_error_code() -> c_int {
    -libc::EAGAIN
}

const fn fferrtag(a: u8, b: u8, c: u8, d: u8) -> c_int {
    -((a as c_int) | ((b as c_int) << 8) | ((c as c_int) << 16) | ((d as c_int) << 24))
}

fn init_network() {
    NETWORK_INIT.call_once(|| unsafe {
        ffi::avformat_network_init();
    });
}

struct FormatContext(*mut ffi::AVFormatContext);

impl Drop for FormatContext {
    fn drop(&mut self) {
        unsafe {
            ffi::avformat_close_input(&mut self.0);
        }
    }
}

struct CodecContext(*mut ffi::AVCodecContext);

impl Drop for CodecContext {
    fn drop(&mut self) {
        unsafe {
            ffi::avcodec_free_context(&mut self.0);
        }
    }
}

struct Packet(*mut ffi::AVPacket);

impl Packet {
    fn new() -> Result<Self, ExtractFramesError> {
        unsafe {
            let ptr = ffi::av_packet_alloc();
            if ptr.is_null() {
                Err(ExtractFramesError::Ffmpeg(
                    "av_packet_alloc returned null".to_string(),
                ))
            } else {
                Ok(Self(ptr))
            }
        }
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe {
            ffi::av_packet_free(&mut self.0);
        }
    }
}

struct Frame(*mut ffi::AVFrame);

impl Frame {
    fn new() -> Result<Self, ExtractFramesError> {
        unsafe {
            let ptr = ffi::av_frame_alloc();
            if ptr.is_null() {
                Err(ExtractFramesError::Ffmpeg(
                    "av_frame_alloc returned null".to_string(),
                ))
            } else {
                Ok(Self(ptr))
            }
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            ffi::av_frame_free(&mut self.0);
        }
    }
}

struct Scaler(*mut ffi::SwsContext);

impl Scaler {
    unsafe fn ensure(
        &mut self,
        src_width: c_int,
        src_height: c_int,
        src_format: c_int,
        dst_width: c_int,
        dst_height: c_int,
    ) -> Result<(), ExtractFramesError> {
        if !self.0.is_null() {
            ffi::sws_freeContext(self.0);
            self.0 = ptr::null_mut();
        }

        self.0 = ffi::sws_getContext(
            src_width,
            src_height,
            src_format as ffi::AVPixelFormat,
            dst_width,
            dst_height,
            ffi::AVPixelFormat_AV_PIX_FMT_RGB24,
            SWS_BILINEAR_FLAG,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        if self.0.is_null() {
            Err(ExtractFramesError::Ffmpeg(
                "sws_getContext returned null".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                ffi::sws_freeContext(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    #[test]
    fn request_validation_rejects_invalid_ranges() {
        let request = ExtractFramesRequest {
            source_uri: "https://example.com/video.mp4".to_string(),
            scan_start_ms: 5_000,
            scan_end_ms: 5_000,
            sample_fps: 1.0,
            frame_width: 224,
            frame_height: 224,
        };

        let error = request.validate().expect_err("expected invalid request");
        assert_eq!(
            error,
            ExtractFramesError::InvalidRequest(
                "scan_end_ms must be greater than scan_start_ms".to_string()
            )
        );
    }

    #[test]
    fn ffmpeg_version_triplet_unpacks_packed_version() {
        let version = FfmpegVersionTriplet::from_packed((62 << 16) | (3 << 8) | 100);
        assert_eq!(version.major, 62);
        assert_eq!(version.minor, 3);
        assert_eq!(version.patch, 100);
    }

    #[test]
    fn linked_ffmpeg_versions_report_non_zero_major_versions() {
        let versions = linked_ffmpeg_versions();
        assert!(versions.avformat.major > 0);
        assert!(versions.avcodec.major > 0);
        assert!(versions.avutil.major > 0);
        assert!(versions.swscale.major > 0);
    }

    #[test]
    fn extract_sampled_rgb_frames_reads_local_test_video() {
        if !video_tools_available() {
            return;
        }

        let dir = temp_test_dir("find-media");
        let input = dir.join("fixture.mp4");
        write_ffmpeg_test_video_custom(&input, 320, 180, 4, "2");

        let response = extract_sampled_rgb_frames(&ExtractFramesRequest {
            source_uri: input.display().to_string(),
            scan_start_ms: 0,
            scan_end_ms: 3_000,
            sample_fps: 1.0,
            frame_width: 224,
            frame_height: 224,
        })
        .expect("extract sampled rgb frames");

        assert!(!response.frames.is_empty());
        assert!(response.frames.len() <= 4);
        assert_eq!(response.frames[0].width, 224);
        assert_eq!(response.frames[0].height, 224);
        assert_eq!(response.frames[0].rgb24.len(), 224 * 224 * 3);
        assert!(response
            .frames
            .windows(2)
            .all(|window| window[0].timestamp_ms < window[1].timestamp_ms));
    }

    fn video_tools_available() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mx8-find-media-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix time")
                .as_millis()
        ));
        std::fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn write_ffmpeg_test_video_custom(
        path: &Path,
        width: u32,
        height: u32,
        duration_secs: u32,
        rate: &str,
    ) {
        let status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg(format!("testsrc=size={}x{}:rate={rate}", width, height))
            .arg("-t")
            .arg(duration_secs.to_string())
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(path)
            .status()
            .expect("run ffmpeg");
        assert!(status.success(), "ffmpeg fixture generation failed");
    }
}
