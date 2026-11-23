//! One line-icon family drawn as local SVG geometry, close to SF Symbols' regular weight. Never a remote asset.
use nsh_core::{element::ImageSrc, prelude::*};
/// The full family is kept for upcoming surfaces (scan sources, sync, shortcuts).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Key,
    Accounts,
    Search,
    Grid,
    List,
    Plus,
    ExportImport,
    Lock,
    Settings,
    Folder,
    FolderPlus,
    FolderMinus,
    More,
    MoreCircle,
    Check,
    CheckCircle,
    Warning,
    Info,
    Back,
    ChevronDown,
    ChevronRight,
    ChevronUpDown,
    Open,
    Shield,
    LockShield,
    Eye,
    EyeSlash,
    Copy,
    Trash,
    Qr,
    QrScan,
    Photo,
    Screen,
    Camera,
    Cloud,
    LockShieldFill,
    ShieldWarningFill,
    CheckCircleFill,
    InfoFill,
    WarningFill,
    FolderFill,
    ServerRack,
    LockFill,
    LockOpenFill,
    CameraViewfinder,
    Refresh,
    ClearFill,
    CloudOff,
    Download,
    Sidebar,
    Close,
    Keyboard,
    Sliders,
    Power,
    Window,
    Doc,
    Hand,
    PlusCircle,
    PlusCircleFill,
    TouchId,
    CloudDownFill,
    CloudDown,
    External,
    DocFill,
    SealFill,
}
fn shape(name: Icon) -> &'static str {
    match name {
        Icon::Accounts => "<rect x='3.5' y='4.5' width='17' height='6' rx='1.6'/><rect x='3.5' y='13.5' width='17' height='6' rx='1.6'/>",
        Icon::Key => "<circle cx='8' cy='8' r='4'/><path d='m11 11 9 9m-4-4 3-3m-6 0 3-3'/>",
        Icon::Search => "<circle cx='10.5' cy='10.5' r='6.5'/><path d='m15.5 15.5 5 5'/>",
        Icon::Grid => {
            "<rect x='3.5' y='3.5' width='7' height='7' rx='1.6'/><rect x='13.5' y='3.5' width='7' height='7' rx='1.6'/><rect x='3.5' y='13.5' width='7' height='7' rx='1.6'/><rect x='13.5' y='13.5' width='7' height='7' rx='1.6'/>"
        }
        Icon::List => {
            "<path d='M9 6h11M9 12h11M9 18h11'/><circle cx='4.5' cy='6' r='.6'/><circle cx='4.5' cy='12' r='.6'/><circle cx='4.5' cy='18' r='.6'/>"
        }
        Icon::Plus => "<path d='M12 5v14M5 12h14'/>",
        Icon::ExportImport => {
            "<path d='M8 8V6.5A1.5 1.5 0 0 1 9.5 5h9A1.5 1.5 0 0 1 20 6.5v9a1.5 1.5 0 0 1-1.5 1.5H17'/><rect x='4' y='8' width='12' height='12' rx='1.5'/><path d='M10 17v-6m-2.5 2.5L10 11l2.5 2.5'/>"
        }
        Icon::Lock => "<rect x='6.5' y='10.5' width='11' height='9' rx='2'/><path d='M9 10.5V7.5a3 3 0 0 1 6 0v3'/>",
        Icon::Settings => {
            "<circle cx='12' cy='12' r='3'/><path d='M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7'/><circle cx='12' cy='12' r='6.6'/>"
        }
        Icon::Folder => {
            "<path d='M3.5 7V6a1.5 1.5 0 0 1 1.5-1.5h4.4l2 2.5h7.6A1.5 1.5 0 0 1 20.5 8.5v9.5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18Z'/><path d='M3.5 9.5h17'/>"
        }
        Icon::FolderPlus => {
            "<path d='M3.5 7V6a1.5 1.5 0 0 1 1.5-1.5h4.4l2 2.5h7.6A1.5 1.5 0 0 1 20.5 8.5v9.5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18Z'/><path d='M12 11v6M9 14h6'/>"
        }
        Icon::FolderMinus => {
            "<path d='M3.5 7V6a1.5 1.5 0 0 1 1.5-1.5h4.4l2 2.5h7.6A1.5 1.5 0 0 1 20.5 8.5v9.5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18Z'/><path d='M9 14h6'/>"
        }
        Icon::More => "<circle cx='5' cy='12' r='1.2'/><circle cx='12' cy='12' r='1.2'/><circle cx='19' cy='12' r='1.2'/>",
        Icon::MoreCircle => {
            "<circle cx='12' cy='12' r='9'/><circle cx='8' cy='12' r='.7'/><circle cx='12' cy='12' r='.7'/><circle cx='16' cy='12' r='.7'/>"
        }
        Icon::Check => "<path d='m5 12.5 4.5 4.5L19 7'/>",
        Icon::CheckCircle => "<circle cx='12' cy='12' r='9'/><path d='m8 12.3 2.8 2.8L16.2 9.5'/>",
        Icon::Warning => "<path d='M12 3.8 21 19.5H3Z'/><path d='M12 10v4.5M12 17.2v.1'/>",
        Icon::Info => "<circle cx='12' cy='12' r='9'/><path d='M12 11v5.5M12 7.6v.1'/>",
        Icon::InfoFill => "<circle cx='12' cy='12' r='9' fill='currentColor'/><path d='M12 11v5.5M12 7.6v.1' stroke='#fff'/>",
        Icon::WarningFill => "<path d='M12 3.8 21 19.5H3Z' fill='currentColor'/><path d='M12 10v4.5M12 17.2v.1' stroke='#fff'/>",
        Icon::Back => "<path d='m11 5-7 7 7 7M4 12h16'/>",
        Icon::ChevronDown => "<path d='m6 9 6 6 6-6'/>",
        Icon::ChevronRight => "<path d='m9 6 6 6-6 6'/>",
        Icon::ChevronUpDown => "<path d='m8 9 4-4 4 4M8 15l4 4 4-4'/>",
        Icon::Open => "<path d='M14 3h7v7M21 3l-9 9M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5'/>",
        Icon::External => "<rect x='3.5' y='3.5' width='17' height='17' rx='3'/><path d='M10 14l6-6M10.5 8H16v5.5'/>",
        Icon::Shield => "<path d='m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z'/>",
        Icon::LockShield => {
            "<path d='m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z'/><rect x='9' y='11' width='6' height='5' rx='1'/><path d='M10.2 11V9.6a1.8 1.8 0 0 1 3.6 0V11'/>"
        }
        Icon::Eye => "<path d='M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z'/><circle cx='12' cy='12' r='3'/>",
        Icon::EyeSlash => {
            "<path d='M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z'/><circle cx='12' cy='12' r='3'/><path d='M4 4l16 16'/>"
        }
        Icon::Copy => {
            "<rect x='8' y='8' width='12' height='12' rx='2'/><path d='M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2'/>"
        }
        Icon::Trash => {
            "<path d='M4.5 6.5h15M9.5 6.5V4.5h5v2M6.5 6.5l.9 12.6a1.5 1.5 0 0 0 1.5 1.4h6.2a1.5 1.5 0 0 0 1.5-1.4l.9-12.6M10 10.5v6M14 10.5v6'/>"
        }
        Icon::Qr => {
            "<rect x='4' y='4' width='6' height='6' rx='1'/><rect x='14' y='4' width='6' height='6' rx='1'/><rect x='4' y='14' width='6' height='6' rx='1'/><path d='M14 14h2v2h-2zM18 18h2v2h-2zM18 14h2M14 18v2'/>"
        }
        Icon::QrScan => {
            "<path d='M4 8V5.5A1.5 1.5 0 0 1 5.5 4H8M16 4h2.5A1.5 1.5 0 0 1 20 5.5V8M20 16v2.5a1.5 1.5 0 0 1-1.5 1.5H16M8 20H5.5A1.5 1.5 0 0 1 4 18.5V16'/><rect x='8' y='8' width='3' height='3'/><rect x='13' y='8' width='3' height='3'/><rect x='8' y='13' width='3' height='3'/><path d='M13 13h3v3'/>"
        }
        Icon::Photo => {
            "<rect x='3.5' y='5' width='17' height='14' rx='2'/><circle cx='9' cy='10' r='1.6'/><path d='m4 17 5-5 4 4 3-3 4 4'/>"
        }
        Icon::Screen => "<rect x='3.5' y='4.5' width='17' height='15' rx='2' stroke-dasharray='3 2.4'/>",
        Icon::Camera => {
            "<path d='M4 8.5A1.5 1.5 0 0 1 5.5 7h2.3l1.4-2h5.6l1.4 2h2.3A1.5 1.5 0 0 1 20 8.5v9A1.5 1.5 0 0 1 18.5 19h-13A1.5 1.5 0 0 1 4 17.5Z'/><circle cx='12' cy='12.5' r='3.4'/>"
        }
        Icon::CloudOff => "<path d='M7 18.5h10.5a3.75 3.75 0 0 0 .4-7.5 6 6 0 0 0-11.5 1.5A3 3 0 0 0 7 18.5Z'/><path d='M4 4l16 16'/>",
        Icon::ClearFill => "<circle cx='12' cy='12' r='8.5'/><path d='M9.2 9.2l5.6 5.6M14.8 9.2l-5.6 5.6'/>",
        Icon::CameraViewfinder => {
            "<path d='M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2'/><rect x='8' y='9' width='8' height='6.5' rx='1.2'/><circle cx='12' cy='12.2' r='1.6'/>"
        }
        // The arc runs clockwise into the arrowhead's corner, along its tangent.
        Icon::Refresh => "<path d='M19 12a7 7 0 1 1-2.05-4.95L19 9'/><path d='M19 5v4h-4'/>",
        Icon::FolderFill => {
            "<path fill='currentColor' d='M3.5 7V6a1.5 1.5 0 0 1 1.5-1.5h4.4l2 2.5h7.6A1.5 1.5 0 0 1 20.5 8.5v9.5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18Z'/>"
        }
        Icon::ServerRack => {
            "<rect x='4' y='4.5' width='16' height='6' rx='1.5'/><rect x='4' y='13.5' width='16' height='6' rx='1.5'/><path d='M7.5 7.5h.01M7.5 16.5h.01'/>"
        }
        Icon::LockFill => {
            "<rect x='6.5' y='10.5' width='11' height='9' rx='2' fill='currentColor'/><path d='M9 10.5V7.5a3 3 0 0 1 6 0v3'/>"
        }
        Icon::LockOpenFill => {
            "<rect x='6.5' y='10.5' width='11' height='9' rx='2' fill='currentColor'/><path d='M9 10.5V7.5a3 3 0 0 1 5.8-1'/>"
        }
        Icon::LockShieldFill => "<path fill='currentColor' d='M12 3l7 3v5.5c0 4.2-2.9 8-7 9.5-4.1-1.5-7-5.3-7-9.5V6z'/>",
        Icon::ShieldWarningFill => {
            "<path fill='currentColor' d='M12 3l7 3v5.5c0 4.2-2.9 8-7 9.5-4.1-1.5-7-5.3-7-9.5V6z'/><path d='M12 8v5' stroke='white'/><circle cx='12' cy='16' r='.6' fill='white' stroke='white'/>"
        }
        Icon::CheckCircleFill => "<circle cx='12' cy='12' r='9' fill='currentColor'/><path d='M8.2 12.3l2.6 2.6 5-5.2' stroke='#fff'/>",
        Icon::Cloud => "<path d='M7 18.5h10.5a3.75 3.75 0 0 0 .4-7.5 6 6 0 0 0-11.5 1.5A3 3 0 0 0 7 18.5Z'/>",
        Icon::CloudDown => {
            "<path d='M7 18.5h10.5a3.75 3.75 0 0 0 .4-7.5 6 6 0 0 0-11.5 1.5A3 3 0 0 0 7 18.5Z'/><path d='M12 10.5v5m-2.2-2.2 2.2 2.2 2.2-2.2'/>"
        }
        Icon::Sidebar => "<rect x='3.5' y='4.5' width='17' height='15' rx='2.5'/><path d='M9.5 4.5v15M5.8 8h1.6M5.8 11h1.6'/>",
        Icon::Close => "<path d='m6.5 6.5 11 11m0-11-11 11'/>",
        Icon::Keyboard => "<rect x='3' y='6' width='18' height='12' rx='2'/><path d='M7 10h.1M10.3 10h.1M13.6 10h.1M17 10h.1M7 14h10'/>",
        Icon::Sliders => "<path d='M4 7h9M17 7h3M4 17h3M11 17h9'/><circle cx='15' cy='7' r='2'/><circle cx='9' cy='17' r='2'/>",
        Icon::Power => "<path d='M12 3.5v8'/><path d='M7.3 6.5a7.5 7.5 0 1 0 9.4 0'/>",
        Icon::Window => "<rect x='3.5' y='4.5' width='17' height='15' rx='2'/><path d='M3.5 8.5h17'/>",
        Icon::Doc => "<path d='M6.5 3.5h7l4 4v12a1 1 0 0 1-1 1h-10a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1Z'/><path d='M13.5 3.5v4h4'/>",
        Icon::Download => "<path d='M12 4v11m-4.5-4.5L12 15l4.5-4.5M4.5 15.5v3a1.5 1.5 0 0 0 1.5 1.5h12a1.5 1.5 0 0 0 1.5-1.5v-3'/>",
        Icon::Hand => {
            "<path d='M8 13V6.5a1.5 1.5 0 0 1 3 0V12m0-6.5V4.5a1.5 1.5 0 0 1 3 0V12m0-5.5a1.5 1.5 0 0 1 3 0V14a6.5 6.5 0 0 1-6.5 6.5h-.6a5 5 0 0 1-4.3-2.4L4.6 15a1.4 1.4 0 0 1 2.3-1.6L8 15'/>"
        }
        Icon::PlusCircle => "<circle cx='12' cy='12' r='9'/><path d='M12 8v8M8 12h8'/>",
        Icon::TouchId => {
            "<path d='M6.2 8.6A7 7 0 0 1 18.4 9'/><path d='M4.5 13.2a7.6 7.6 0 0 1 .3-2.6M19.4 11.8c.1.9.1 1.8 0 2.7'/><path d='M8 18.6a9.6 9.6 0 0 1-1.1-4.6 5.1 5.1 0 0 1 10.2 0v.6'/><path d='M10.5 20a12 12 0 0 1-1.1-5.9 2.6 2.6 0 0 1 5.2 0c0 1.7.1 3.4.6 5'/><path d='M12 14.2c0 2.3.4 4.3 1.3 6'/><path d='M17 17.3c-.2 1-.3 1.9-.2 2.6'/>"
        }
        Icon::PlusCircleFill => {
            "<circle cx='12' cy='12' r='10' fill='currentColor' stroke='none'/><path d='M12 7.5v9M7.5 12h9' stroke='#fff' stroke-width='2.2'/>"
        }
        Icon::DocFill => "<path fill='currentColor' d='M6.5 3.5h7l4 4v12a1 1 0 0 1-1 1h-10a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1Z'/>",
        // Twelve even points around the check, their tips rounded by the stroke.
        Icon::SealFill => {
            "<path fill='currentColor' stroke='currentColor' stroke-width='1.2' d='M12 2.6L14.04 4.37L16.7 3.86L17.59 6.41L20.14 7.3L19.63 9.96L21.4 12L19.63 14.04L20.14 16.7L17.59 17.59L16.7 20.14L14.04 19.63L12 21.4L9.96 19.63L7.3 20.14L6.41 17.59L3.86 16.7L4.37 14.04L2.6 12L4.37 9.96L3.86 7.3L6.41 6.41L7.3 3.86L9.96 4.37Z'/><path d='m8.6 12.2 2.3 2.3 4.5-4.6' stroke='#fff' stroke-width='2'/>"
        }
        Icon::CloudDownFill => {
            "<path d='M6.8 19h11a4.2 4.2 0 0 0 .5-8.4 6.5 6.5 0 0 0-12.5 1.6A3.4 3.4 0 0 0 6.8 19Z' fill='currentColor' stroke='none'/><path d='M12 10.5v6m-2.6-2.6L12 16.5l2.6-2.6' stroke='#fff' stroke-width='2'/>"
        }
    }
}
pub fn icon(name: Icon) -> impl IntoElement + Styled {
    weighted(name, 1.6)
}
/// The SF Symbol for a glyph, where it has one.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn sf_name(name: Icon) -> Option<&'static str> {
    Some(match name {
        Icon::Accounts => "rectangle.grid.1x2",
        Icon::TouchId => "touchid",
        Icon::Folder => "folder",
        Icon::FolderPlus => "folder.badge.plus",
        Icon::ExportImport => "square.and.arrow.up.on.square",
        Icon::Cloud => "icloud",
        Icon::CloudOff => "icloud.slash",
        Icon::Download => "square.and.arrow.down",
        Icon::Settings => "gearshape",
        Icon::Sidebar => "sidebar.left",
        Icon::Lock => "lock",
        Icon::List => "list.bullet",
        Icon::Grid => "square.grid.2x2",
        Icon::Search => "magnifyingglass",
        Icon::Photo => "photo.on.rectangle",
        Icon::Eye => "eye",
        Icon::EyeSlash => "eye.slash",
        Icon::LockShieldFill => "lock.shield.fill",
        Icon::ShieldWarningFill => "exclamationmark.shield.fill",
        Icon::CheckCircleFill => "checkmark.circle.fill",
        Icon::InfoFill => "info.circle.fill",
        Icon::WarningFill => "exclamationmark.triangle.fill",
        Icon::FolderFill => "folder.fill",
        Icon::DocFill => "doc.fill",
        Icon::SealFill => "checkmark.seal.fill",
        Icon::ServerRack => "server.rack",
        Icon::LockFill => "lock.fill",
        Icon::LockOpenFill => "lock.open.fill",
        Icon::QrScan => "qrcode.viewfinder",
        Icon::Camera => "camera.viewfinder",
        Icon::CameraViewfinder => "camera.viewfinder",
        Icon::Refresh => "arrow.clockwise",
        Icon::Screen => "rectangle.dashed",
        Icon::LockShield => "lock.shield",
        Icon::CheckCircle => "checkmark.circle",
        Icon::ClearFill => "xmark.circle.fill",
        Icon::External => "arrow.up.forward.square",
        Icon::Keyboard => "keyboard",
        Icon::Sliders => "slider.horizontal.3",
        Icon::ChevronUpDown => "chevron.up.chevron.down",
        Icon::MoreCircle => "ellipsis.circle",
        Icon::Check => "checkmark",
        Icon::Info => "info.circle",
        Icon::Close => "xmark",
        Icon::ChevronRight => "chevron.right",
        Icon::ChevronDown => "chevron.down",
        Icon::Hand => "hand.point.up.left",
        _ => return None,
    })
}
/// A glyph at SF Symbols' `pt` point size, regular weight, in a square box of `pt × 24/17` (the line glyphs' frame).
/// macOS draws the real SF Symbol (`platform::symbols`, outside NSH); other desktops and unmapped glyphs use the
/// local SVG line icon. `color` tints it: a template image does not inherit the text color.
pub fn symbol(name: Icon, pt: f32, color: impl Fn() -> AdaptiveColor + Copy + 'static) -> impl IntoElement + Styled {
    symbol_weight(name, pt, 0.0, color)
}
/// As [`symbol`], at an AppKit font weight (medium 0.23, semibold 0.3); the SVG fallback keeps its regular line.
pub fn symbol_weight(name: Icon, pt: f32, weight: f32, color: impl Fn() -> AdaptiveColor + Copy + 'static) -> impl IntoElement + Styled {
    let _ = weight;
    let side = pt * 24.0 / 17.0;
    let frame = view().size(px(side)).items_center().justify_center();
    #[cfg(target_os = "macos")]
    if let Some(r) = sf_name(name).and_then(|n| crate::platform::symbols::render(n, pt, weight)) {
        return frame.child(image(r.uri).w(px(r.width)).h(px(r.height)).overflow_hidden().tint_color(color).into_element());
    }
    frame.text_color(color).child(weighted(name, 1.25).size(px(side)).into_element())
}
/// The Touch ID fingerprint, `height` points tall, in the Touch ID pink: the system's `touchid` symbol on macOS, as the
/// system's own Touch ID prompts draw it (filling about 70% of the circle it sits in); elsewhere the SVG glyph scaled up
/// by its own padding (its strokes span about 16 of its 24 units) and drawn heavier.
pub fn touch_id_glyph(height: f32) -> impl IntoElement + Styled {
    let color = move || crate::components::tok("touchid");
    let frame = view().size(px(height)).items_center().justify_center();
    #[cfg(target_os = "macos")]
    {
        // The symbol's height per point size, from one rendering; then the point size that gives `height`.
        let fit = crate::platform::symbols::render("touchid", 20.0, 0.0)
            .and_then(|r| crate::platform::symbols::render("touchid", 20.0 * height / r.height, 0.0));
        if let Some(r) = fit {
            return frame.child(image(r.uri).w(px(r.width)).h(px(r.height)).tint_color(color).into_element());
        }
    }
    frame.text_color(color).child(weighted(Icon::TouchId, 1.9).size(px(height * 24.0 / 16.0)).into_element())
}
/// A heavier stroke, as SF Symbols' semibold (the onboarding back arrow).
pub fn icon_bold(name: Icon) -> impl IntoElement + Styled {
    weighted(name, 2.2)
}
fn weighted(name: Icon, width: f32) -> impl IntoElement + Styled {
    svg(ImageSrc::svg_markup(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='{width}' stroke-linecap='round' stroke-linejoin='round'>{}</svg>",
        shape(name)
    )))
    .size_4()
}
