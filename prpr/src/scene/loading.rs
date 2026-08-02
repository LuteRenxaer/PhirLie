use super::{draw_background, ending::RecordUpdateState, game::GameMode, GameScene, NextScene, Scene};
use crate::{
    config::Config,
    core::Resource,
    ext::{poll_future, semi_black, semi_white, LocalTask, SafeTexture, BLACK_TEXTURE},
    fs::FileSystem,
    info::ChartInfo,
    judge::Judge,
    scene::game::SimpleRecord,
    task::Task,
    time::TimeManager,
    ui::{clip_rounded_rect, rounded_rect_shadow, LoadingParams, ShadowConfig, Ui, PREFER_REDUCED_MOTION},
};
use ::rand::{seq::SliceRandom, thread_rng};
use anyhow::{Context, Result};
use macroquad::prelude::*;
use regex::Regex;
use std::sync::{atomic::Ordering, Arc};
use tracing::warn;

const BEFORE_TIME: f32 = 1.;
const FADE_IN_TIME: f32 = 0.6;

pub type UploadFn = Arc<dyn Fn(Vec<u8>) -> Task<Result<RecordUpdateState>>>;
pub type UpdateFn = Box<dyn FnMut(f64, &mut Resource, &mut Judge)>;
pub type SaveFn = Box<dyn Fn(SimpleRecord) -> Result<()>>;

fn transition_time() -> Option<f32> {
    if PREFER_REDUCED_MOTION.load(Ordering::Relaxed) {
        None
    } else {
        Some(1.4)
    }
}

fn wait_time() -> f32 {
    if PREFER_REDUCED_MOTION.load(Ordering::Relaxed) {
        0.
    } else {
        0.4
    }
}

pub struct BasicPlayer {
    pub avatar: Option<SafeTexture>,
    pub id: i32,
    pub rks: f32,
    pub historic_best: u32,
}

pub struct LoadingScene {
    info: ChartInfo,
    background: SafeTexture,
    illustration: SafeTexture,
    pub load_task: LocalTask<Result<GameScene>>,
    next_scene: Option<NextScene>,
    finish_time: f32,
    target: Option<RenderTarget>,
    charter: String,

    theme_color: Color,
    use_black: bool,
}

impl LoadingScene {
    pub async fn load(fs: &mut dyn FileSystem, path: &str) -> Result<(SafeTexture, SafeTexture, Color)> {
        let image = image::load_from_memory(&fs.load_file(path).await?).context("Failed to decode image")?;
        let (w, h) = (image.width(), image.height());
        let size = w as usize * h as usize;

        let mut blurred_rgb = image.to_rgb8();
        let color = color_thief::get_palette(&blurred_rgb, color_thief::ColorFormat::Rgb, 10, 2)?[0];
        let mut vec = unsafe { Vec::from_raw_parts(std::mem::transmute::<*mut u8, *mut [u8; 3]>(blurred_rgb.as_mut_ptr()), size, size) };
        fastblur::gaussian_blur(&mut vec, w as _, h as _, 50.);
        std::mem::forget(vec);
        let mut blurred = Vec::with_capacity(size * 4);
        for input in blurred_rgb.chunks_exact(3) {
            blurred.extend_from_slice(input);
            blurred.push(255);
        }
        Ok((
            Texture2D::from_rgba8(w as _, h as _, &image.into_rgba8()).into(),
            Texture2D::from_image(&Image {
                width: w as _,
                height: h as _,
                bytes: blurred,
            })
            .into(),
            Color::from_rgba(color.r, color.g, color.b, 255),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mode: GameMode,
        mut info: ChartInfo,
        config: Config,
        mut fs: Box<dyn FileSystem>,
        player: Option<BasicPlayer>,
        upload_fn: Option<UploadFn>,
        update_fn: Option<UpdateFn>,
        save_fn: Option<SaveFn>,

        preloaded: Option<(SafeTexture, SafeTexture, Color)>,
    ) -> Result<Self> {
        let (background, theme_color) = match preloaded {
            Some((ill, bg, color)) => (Some((ill, bg)), color),
            None => match Self::load(fs.as_mut(), &info.illustration).await {
                Ok((ill, bg, color)) => (Some((ill, bg)), color),
                Err(err) => {
                    warn!("failed to load background: {err:?}");
                    (None, WHITE)
                }
            },
        };
        let use_black = (theme_color.r * 0.299 + theme_color.g * 0.587 + theme_color.b * 0.114) > 186. / 255.;
        let (illustration, background) = background.unwrap_or_else(|| (BLACK_TEXTURE.clone(), BLACK_TEXTURE.clone()));
        if info.tip.is_none() {
            info.tip = Some(crate::config::TIPS.choose(&mut thread_rng()).unwrap().to_owned());
        }
        let future =
            Box::pin(GameScene::new(mode, info.clone(), config, fs, player, background.clone(), illustration.clone(), upload_fn, update_fn, save_fn));
        let charter = Regex::new(r"\[!:[0-9]+:([^:]*)\]").unwrap().replace_all(&info.charter, "$1").to_string();

        Ok(Self {
            info,
            background,
            illustration,
            load_task: Some(future),
            next_scene: None,
            finish_time: f32::INFINITY,
            target: None,
            charter,

            theme_color,
            use_black,
        })
    }
}

impl Scene for LoadingScene {
    fn enter(&mut self, tm: &mut TimeManager, target: Option<RenderTarget>) -> Result<()> {
        self.target = target;
        tm.reset();
        Ok(())
    }

    fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        if let Some(future) = self.load_task.as_mut() {
            loop {
                match poll_future(future.as_mut()) {
                    None => {
                        if self.target.is_none() {
                            break;
                        }
                        std::thread::yield_now();
                    }
                    Some(game_scene) => {
                        self.load_task = None;
                        self.next_scene =
                            Some(game_scene.map_or_else(|e| NextScene::PopWithResult(Box::new(e)), |it| NextScene::Replace(Box::new(it))));
                        self.finish_time = tm.now() as f32 + BEFORE_TIME;
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) -> Result<()> {
        let mut cam = ui.camera();
        let asp = -cam.zoom.y;
        let top = 1. / asp;
        let t = tm.now() as f32;
        cam.render_target = self.target;
        set_camera(&cam);

        // 1. 全屏模糊背景（去掉全局黑色遮罩）
        draw_background(*self.background);

        ui.alpha((t / FADE_IN_TIME).min(1.), |ui| {
            // 进出过渡：整体横向位移
            let slide_offset = if t > self.finish_time {
                transition_time().map_or(1., |tt| {
                    let p = ((t - self.finish_time) / tt).min(1.);
                    p.powi(3) * 1.5
                })
            } else {
                0.
            };
            ui.dx(slide_offset);

            // 卡片：垂直居中
            let card_w = 1.65;
            let card_h = 0.38;
            let card_x = -card_w / 2.0;
            let card_y = -card_h / 2.0;

            let card_rect = Rect {
                x: card_x,
                y: card_y,
                w: card_w,
                h: card_h,
            };

            // 左侧信息区、右侧曲绘区
            let info_width = card_w * 0.38;
            let info_rect = Rect {
                x: card_x,
                y: card_y,
                w: info_width,
                h: card_h,
            };
            let cover_rect = Rect {
                x: card_x + info_width,
                y: card_y,
                w: card_w - info_width,
                h: card_h,
            };

            // 顶部主题色细条（保留）
            let theme_bar_height = 0.012;
            let theme_bar_rect = Rect {
                x: card_x,
                y: card_y - theme_bar_height,
                w: card_w,
                h: theme_bar_height,
            };

            // 阴影配置（圆角阴影）
            let shadow_config = ShadowConfig {
                radius: 0.006,
                ..Default::default()
            };
            rounded_rect_shadow(ui, card_rect, &shadow_config);

            clip_rounded_rect(ui, card_rect, shadow_config.radius, |ui| {
                // 去掉卡片本身的黑色背景填充（原为 semi_black(0.82)）
                // 只绘制主题色细条
                ui.fill_rect(theme_bar_rect, self.theme_color);

                // 左侧文字区域（无背景，直接显示在模糊背景上）
                let padding = 0.035;
                let text_left = info_rect.x + padding;
                let mid_y = info_rect.y + info_rect.h * 0.5;

                // 曲名
                ui.text(&self.info.name)
                    .pos(text_left, mid_y - 0.055)
                    .anchor(0., 0.5)
                    .size(0.65)
                    .color(WHITE)
                    .max_width(info_rect.w - padding * 2.)
                    .draw();
                // 曲师
                ui.text(&self.info.composer)
                    .pos(text_left, mid_y - 0.015)
                    .anchor(0., 0.5)
                    .size(0.38)
                    .color(semi_white(0.65))
                    .max_width(info_rect.w - padding * 2.)
                    .draw();
                // 谱师
                ui.text(&format!("Chart by {}", self.charter))
                    .pos(text_left, mid_y + 0.025)
                    .anchor(0., 0.5)
                    .size(0.38)
                    .color(semi_white(0.65))
                    .max_width(info_rect.w - padding * 2.)
                    .draw();
                // 画师
                ui.text(&format!("Illustration by {}", self.info.illustrator))
                    .pos(text_left, mid_y + 0.065)
                    .anchor(0., 0.5)
                    .size(0.38)
                    .color(semi_white(0.65))
                    .max_width(info_rect.w - padding * 2.)
                    .draw();

                // 右侧曲绘
                ui.fill_rect(cover_rect, (*self.illustration, cover_rect));

                // 曲绘左侧渐变遮罩（从黑色到透明，让左侧文字更清晰）
                ui.fill_rect(
                    Rect {
                        x: cover_rect.x,
                        y: cover_rect.y,
                        w: 0.08,
                        h: cover_rect.h,
                    },
                    Color::new(0., 0., 0., 0.7),
                );
            });

            // 左下角提示文字
            ui.text(self.info.tip.as_ref().unwrap())
                .pos(-0.95, top - 0.06)
                .anchor(0., 1.)
                .size(0.45)
                .color(semi_white(0.55))
                .draw();

            // 右下角加载圈
            let loading_alpha = if t > self.finish_time {
                let p = ((t - self.finish_time) / 0.4).min(1.);
                (1. - p).powi(3)
            } else {
                1.
            };
            ui.loading(
                0.92,
                top - 0.06,
                t,
                semi_white(loading_alpha * 0.75),
                LoadingParams {
                    radius: 0.035,
                    width: 0.008,
                    ..Default::default()
                },
            );
        });

        Ok(())
    }

    fn next_scene(&mut self, tm: &mut TimeManager) -> NextScene {
        if matches!(self.next_scene, Some(NextScene::PopWithResult(_))) {
            return self.next_scene.take().unwrap();
        }
        if tm.now() as f32 > self.finish_time + transition_time().unwrap_or_default() + wait_time() {
            if let Some(scene) = self.next_scene.take() {
                return scene;
            }
        }
        NextScene::None
    }
}