//! metatorio-icons：在**自己的进程内**解析并渲染 Factorio 图标。
//!
//! 为什么不用游戏的 `--dump-icon-sprites`：Steam 版启动时要用户确认（DRM 启动流程），
//! 自动导出流程会被这一下打断；而且图标本来就是「读几个 PNG + 按 IconData 规则叠加」，
//! 完全可以自己算。
//!
//! 分层：
//! - [`sources`]：把 `__base__/…`、`__core__/…`、`__mod__/…` 解析到真实文件（目录或
//!   mod 的 zip）；
//! - [`image`]：RGBA8 缓冲、PNG 解码/编码、直通↔预乘转换、重采样与合成；
//! - [`render`]：按官方 `IconData` 规则把一层或多层叠成一张图标；
//! - [`compare`]：和游戏自己导出的图标逐像素比对（验证用，也是本 crate 的验收手段）。
//!
//! **alpha 约定**：crate 内部一律用**直通 alpha**（straight，和游戏源 PNG 一致）；
//! 游戏 `--dump-icon-sprites` 导出的是**预乘 alpha**，所以比对时要把我们这边乘一次
//! （见 [`compare`]）。

pub mod compare;
pub mod image;
pub mod render;
pub mod sources;

pub use image::Rgba8;
pub use render::{
    IconRenderError, RenderOptions, RenderReport, ScaleLaw, render_all_icons, render_icon,
    render_prototype_icon, render_prototype_icon_with,
};
pub use sources::{Archive, IconSources};
