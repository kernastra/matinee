//! FFmpeg-direct probe (via `ffmpeg-next`): demux + decode + swscale to BGRA
//! as fast as possible, then seek. This is the *bottom* of a player only:
//! audio output, A/V sync, clocking, subtitle rendering, HLS session
//! handling and hardware-decode interop would all have to be written on top.
//!
//! ffmpeg-probe <path-or-url> [--frames N] [--seek SECS]

use std::time::Instant;

use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::software::scaling::{Context as Scaler, Flags};
use native_playback_spike::frame::Timing;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut source, mut frames, mut seek) = (String::new(), 240usize, None::<f64>);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--frames" => frames = args.next().context("--frames N")?.parse()?,
            "--seek" => seek = Some(args.next().context("--seek SECS")?.parse()?),
            _ => source = arg,
        }
    }
    anyhow::ensure!(
        !source.is_empty(),
        "usage: ffmpeg-probe <path-or-url> [--frames N] [--seek SECS]"
    );

    ffmpeg::init()?;
    println!(
        "INFO libavcodec: {}.{}.{} configuration: {}",
        ffmpeg::codec::version() >> 16,
        (ffmpeg::codec::version() >> 8) & 0xff,
        ffmpeg::codec::version() & 0xff,
        ffmpeg::codec::configuration()
    );
    println!("INFO license: {}", ffmpeg::codec::license());

    let opened = Instant::now();
    let mut input = ffmpeg::format::input(&source)?;
    for stream in input.streams() {
        let parameters = stream.parameters();
        let language = stream.metadata().get("language").unwrap_or("-").to_string();
        println!(
            "INFO stream: #{} {:?} {:?} lang={language}",
            stream.index(),
            parameters.medium(),
            parameters.id()
        );
    }
    let video = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .context("no video stream")?;
    let video_index = video.index();
    let time_base = f64::from(video.time_base());
    let mut decoder = ffmpeg::codec::context::Context::from_parameters(video.parameters())?
        .decoder()
        .video()?;
    println!(
        "INFO video: {:?} {}x{} {:?} open={:?}",
        decoder.id(),
        decoder.width(),
        decoder.height(),
        decoder.format(),
        opened.elapsed()
    );

    if let Some(target) = seek {
        let began = Instant::now();
        let timestamp = (target * f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
        input.seek(timestamp, ..timestamp)?;
        decoder.flush();
        println!(
            "INFO seek: av_seek_frame to <= {target}s took {:?} (keyframe; exact needs decode-forward)",
            began.elapsed()
        );
    }

    let mut scaler: Option<Scaler> = None;
    let (mut decode, mut convert) = (Timing::default(), Timing::default());
    let mut decoded = ffmpeg::frame::Video::empty();
    let mut bgra = ffmpeg::frame::Video::empty();
    let mut first_pts = None;
    let wall = Instant::now();
    let mut done = 0usize;
    let mut last_packet = Instant::now();

    'packets: for (stream, packet) in input.packets() {
        if stream.index() != video_index {
            continue;
        }
        decoder.send_packet(&packet)?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            decode.record(last_packet.elapsed());
            first_pts.get_or_insert(decoded.timestamp().unwrap_or(0) as f64 * time_base);
            let scaler = match &mut scaler {
                Some(scaler) => scaler,
                None => scaler.insert(Scaler::get(
                    decoded.format(),
                    decoded.width(),
                    decoded.height(),
                    Pixel::BGRA,
                    decoded.width(),
                    decoded.height(),
                    Flags::BILINEAR,
                )?),
            };
            let began = Instant::now();
            scaler.run(&decoded, &mut bgra)?;
            convert.record(began.elapsed());
            done += 1;
            last_packet = Instant::now();
            if done >= frames {
                break 'packets;
            }
        }
    }
    let elapsed = wall.elapsed().as_secs_f64();
    println!("INFO first-decoded-pts: {:.3}s", first_pts.unwrap_or(-1.0));
    println!(
        "PASS decode: {done} frames in {elapsed:.2}s = {:.0} fps (single decoder, no A/V sync)",
        done as f64 / elapsed
    );
    println!("INFO decode-cost: {}", decode.summary());
    println!("INFO swscale-bgra-cost: {}", convert.summary());
    Ok(())
}
