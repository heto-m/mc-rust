use std::path::PathBuf;

use mcr_render::Camera;
use mcr_render::glam::DVec3;

pub const DEFAULT_JAR: &str = ".cache/versions/26.3/client.jar";
pub const DEFAULT_ASSETS: &str = ".cache/assets";

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
    pub connect: Option<String>,
    pub name: String,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub after_ticks: Option<u32>,
    pub autopilot: Option<PathBuf>,
    pub render_distance_set: bool,
    pub bench_frames: u32,
    pub bench_views: Vec<Camera>,
    pub bench_modes: bool,
    pub frames: u32,
    pub frame_step: Option<Camera>,
    pub occlusion: bool,
    pub visibility_graph: bool,
    pub mods: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            world: None,
            synthetic: false,
            radius: 24,
            render_distance: 32,
            vsync: true,
            jar: PathBuf::from(DEFAULT_JAR),
            screenshot: None,
            camera: None,
            size: (1920, 1080),
            autofly: None,
            connect: None,
            name: "Player".into(),
            game_dir: PathBuf::from("run/client"),
            assets_dir: PathBuf::from(DEFAULT_ASSETS),
            after_ticks: None,
            autopilot: None,
            render_distance_set: false,
            bench_frames: 0,
            bench_views: Vec::new(),
            bench_modes: false,
            frames: 0,
            frame_step: None,
            occlusion: true,
            visibility_graph: true,
            mods: None,
        }
    }
}

pub const USAGE: &str = "usage: mcr [--world <save> | --synthetic | --connect host[:port]] [--render-distance N] [--no-vsync] [--no-occlusion] [--no-graph]
           [--jar <client.jar>] [--screenshot out.png [--camera x,y,z,yaw,pitch] [--size WxH] [--frames N [--frame-step dx,dy,dz,dyaw,dpitch]]]
           [--autofly blocks_per_s] [--bench frames [--bench-view x,y,z,yaw,pitch]... [--bench-modes]]
       connect mode: [--name <name>] [--game-dir dir] [--assets-dir dir] [--autopilot script]
           [--screenshot out.png [--after-ticks N]]
       mods: [--mods <dir>] uses <dir> instead of <game-dir>/mods; enabled mods boot under Fabric (needs `cargo xtask fabric-fetch`)
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
            "--no-occlusion" => opts.occlusion = false,
            "--no-graph" => opts.visibility_graph = false,
            "--bench-modes" => opts.bench_modes = true,
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
            "--connect" => opts.connect = Some(value(&arg, &mut args)?),
            "--name" => opts.name = value(&arg, &mut args)?,
            "--game-dir" => opts.game_dir = value(&arg, &mut args)?.into(),
            "--assets-dir" => opts.assets_dir = value(&arg, &mut args)?.into(),
            "--mods" => opts.mods = Some(value(&arg, &mut args)?.into()),
            "--autopilot" => opts.autopilot = Some(value(&arg, &mut args)?.into()),
            "--after-ticks" => {
                opts.after_ticks = Some(
                    value(&arg, &mut args)?
                        .parse()
                        .map_err(|_| "--after-ticks needs a tick count")?,
                );
            }
            "--bench" => {
                opts.bench_frames = value(&arg, &mut args)?
                    .parse()
                    .map_err(|_| "--bench needs a frame count")?;
            }
            "--frames" => {
                opts.frames = value(&arg, &mut args)?
                    .parse()
                    .ok()
                    .filter(|n| *n <= 1000)
                    .ok_or("--frames needs a count up to 1000")?;
            }
            "--frame-step" => opts.frame_step = Some(parse_camera(&value(&arg, &mut args)?)?),
            "--bench-view" => opts
                .bench_views
                .push(parse_camera(&value(&arg, &mut args)?)?),
            "--size" => opts.size = parse_size(&value(&arg, &mut args)?)?,
            "--render-distance" => {
                opts.render_distance = value(&arg, &mut args)?
                    .parse()
                    .ok()
                    .filter(|d| (1..=128).contains(d))
                    .ok_or("--render-distance must be 1..=128")?;
                opts.render_distance_set = true;
            }
            "--radius" => {
                opts.radius = value(&arg, &mut args)?
                    .parse()
                    .map_err(|_| "--radius must be a number")?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if opts.synthetic && opts.screenshot.is_some() {
        return Err("--screenshot needs --world or the menu flow".into());
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
    fn connect_mode_options() {
        let o = parse_str(
            "--connect localhost:25590 --name Bot --game-dir g --autopilot s.txt --screenshot o.png --after-ticks 40",
        )
        .unwrap();
        assert_eq!(o.connect.as_deref(), Some("localhost:25590"));
        assert_eq!(o.name, "Bot");
        assert_eq!(o.game_dir, PathBuf::from("g"));
        assert_eq!(o.after_ticks, Some(40));
        assert!(o.autopilot.is_some());
    }

    #[test]
    fn mods_folder() {
        let o = parse_str("--mods run/mods").unwrap();
        assert_eq!(o.mods, Some(PathBuf::from("run/mods")));
        assert!(parse_str("--mods").is_err());
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
    fn bench_views_accumulate() {
        let o = parse_str(
            "--world w --screenshot o.png --bench 100 --bench-view 0,70,0,0,0 --bench-view 0,20,0,90,0",
        )
        .unwrap();
        assert_eq!(o.bench_frames, 100);
        assert_eq!(o.bench_views.len(), 2);
        assert_eq!(o.bench_views[1].position.y, 20.0);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_str("").is_ok());
        assert!(parse_str("--world").is_err());
        assert!(parse_str("--world w --camera 1,2").is_err());
        assert!(parse_str("--world w --render-distance 0").is_err());
        assert!(parse_str("--synthetic --screenshot o.png").is_err());
        assert!(parse_str("--connect").is_err());
        assert!(parse_str("--synthetic").is_ok());
    }
}
