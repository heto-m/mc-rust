use std::path::PathBuf;

use mcr_render::Camera;
use mcr_render::glam::DVec3;

pub const DEFAULT_JAR: &str = ".cache/versions/26.3/client.jar";

#[derive(Debug, Clone)]
pub struct Options {
    pub world: Option<PathBuf>,
    pub synthetic: bool,
    pub radius: i32,
    pub render_distance: i32,
    pub vsync: bool,
    pub jar: PathBuf,
    pub screenshot: Option<PathBuf>,
    pub camera: Option<Camera>,
    pub size: (u32, u32),
    pub autofly: Option<f64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            world: None,
            synthetic: false,
            radius: 24,
            render_distance: 16,
            vsync: true,
            jar: PathBuf::from(DEFAULT_JAR),
            screenshot: None,
            camera: None,
            size: (1920, 1080),
            autofly: None,
        }
    }
}

pub const USAGE: &str = "usage: mcr (--world <save> | --synthetic) [--render-distance N] [--no-vsync] [--jar <client.jar>]
           [--screenshot out.png [--camera x,y,z,yaw,pitch] [--size WxH]] [--autofly blocks_per_s]
camera angles are degrees; yaw 0 looks toward -Z, positive turns left";

fn parse_camera(text: &str) -> Result<Camera, String> {
    let v: Vec<f64> = text
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("bad --camera value: {e}"))?;
    let [x, y, z, yaw, pitch] = v[..] else {
        return Err("--camera needs x,y,z,yaw,pitch".into());
    };
    Ok(Camera {
        position: DVec3::new(x, y, z),
        yaw: (yaw as f32).to_radians(),
        pitch: (pitch as f32).to_radians(),
        ..Camera::default()
    })
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    text.split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|&(w, h)| w > 0 && h > 0)
        .ok_or_else(|| "--size needs WIDTHxHEIGHT".to_owned())
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut args = args.into_iter();
    let value = |name: &str, args: &mut dyn Iterator<Item = String>| {
        args.next().ok_or_else(|| format!("{name} needs a value"))
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--world" => opts.world = Some(value(&arg, &mut args)?.into()),
            "--synthetic" => opts.synthetic = true,
            "--no-vsync" => opts.vsync = false,
            "--jar" => opts.jar = value(&arg, &mut args)?.into(),
            "--screenshot" => opts.screenshot = Some(value(&arg, &mut args)?.into()),
            "--camera" => opts.camera = Some(parse_camera(&value(&arg, &mut args)?)?),
            "--autofly" => {
                opts.autofly = Some(
                    value(&arg, &mut args)?
                        .parse()
                        .map_err(|_| "--autofly needs blocks per second")?,
                );
            }
            "--size" => opts.size = parse_size(&value(&arg, &mut args)?)?,
            "--render-distance" => {
                opts.render_distance = value(&arg, &mut args)?
                    .parse()
                    .ok()
                    .filter(|d| (1..=128).contains(d))
                    .ok_or("--render-distance must be 1..=128")?;
            }
            "--radius" => {
                opts.radius = value(&arg, &mut args)?
                    .parse()
                    .map_err(|_| "--radius must be a number")?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if opts.world.is_none() && !opts.synthetic {
        return Err("give --world <save> or --synthetic".into());
    }
    if opts.screenshot.is_some() && opts.world.is_none() {
        return Err("--screenshot needs --world".into());
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Result<Options, String> {
        parse(s.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn world_with_options() {
        let o = parse_str("--world w --render-distance 32 --no-vsync --jar a.jar").unwrap();
        assert_eq!(o.world.as_deref(), Some("w".as_ref()));
        assert_eq!(o.render_distance, 32);
        assert!(!o.vsync);
        assert_eq!(o.jar, PathBuf::from("a.jar"));
    }

    #[test]
    fn camera_angles_are_degrees() {
        let o = parse_str("--world w --screenshot o.png --camera 1,2,3,90,-45").unwrap();
        let c = o.camera.unwrap();
        assert_eq!(c.position, DVec3::new(1.0, 2.0, 3.0));
        assert!((c.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((c.pitch + std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_str("").is_err());
        assert!(parse_str("--world").is_err());
        assert!(parse_str("--world w --camera 1,2").is_err());
        assert!(parse_str("--world w --render-distance 0").is_err());
        assert!(parse_str("--synthetic --screenshot o.png").is_err());
        assert!(parse_str("--synthetic").is_ok());
    }
}
