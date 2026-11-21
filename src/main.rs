#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod attention;
mod colordb;
mod control;
mod drag;
mod bluetooth;
mod brightness;
mod calendar;
mod glass;
mod guard;
mod i18n;
mod launcher;
mod media;
mod panel;
mod power;
mod render;
mod stacks;
mod status;
mod taskbar;
mod tint;
mod topbar;
mod tray;
mod overflow;
mod volume;
mod wifi;

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tiny_skia::Pixmap;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, HDC, HGDIOBJ, ScreenToClient, SelectObject,
};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmFlush, DwmGetWindowAttribute};
use windows::core::BOOL;
use windows::Win32::Foundation::{FALSE, TRUE};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::core::{HSTRING, PCWSTR, w};

use apps::App;
use render::{Metrics, Theme};

const TIMER_REFRESH: usize = 2;
const TIMER_EDGE: usize = 3;
const TIMER_GLASS: usize = 4;
/// Quanto tempo o dock continua visível depois que o motivo para ficar visível acabou.
const HIDE_DELAY: Duration = Duration::from_millis(500);
/// Tempo de subida e descida do dock ao aparecer/sumir (segundos).
const SLIDE_TIME: f32 = 0.18;
/// Tempo de entrada/saída do zoom (segundos).
const HOVER_TIME: f32 = 0.14;
const MENU_QUIT: usize = 1;
const MENU_PIN: usize = 2;
const MENU_CLOSE: usize = 3;
/// Itens das janelas do app: MENU_WINDOW + índice.
const MENU_WINDOW: usize = 100;
const WM_MOUSELEAVE: u32 = 0x02A3;
/// Abre/fecha o lançador. Mandada de fora (atalho do AutoHotkey) para a janela do dock
/// (classe `ArcDockWin`): `PostMessage 0x8032`.
const WM_LAUNCHER: u32 = WM_APP + 50;

thread_local! {
    static DOCK: RefCell<Option<Dock>> = const { RefCell::new(None) };
}

struct Dock {
    hwnd: HWND,
    /// Janela atrás dos ícones que só fornece o desfoque (vidro).
    glass: HWND,
    /// Último retângulo aplicado ao vidro (com o deslocamento vertical), para não repetir.
    glass_rect: Option<(i32, i32, i32, i32)>,
    glass_fx: Option<glass::Glass>,
    m: Metrics,
    theme: Theme,
    icon_size: i32,
    /// Data do arquivo de ícones trocados, para recarregar quando ele mudar.
    icons_mtime: Option<std::time::SystemTime>,
    apps: Vec<App>,
    /// Downloads e Lixeira, na ponta direita depois de um separador (stacks.rs).
    stacks: stacks::Stacks,
    pixmap: Pixmap,
    memdc: HDC,
    bits: *mut u8,
    pos: POINT,
    /// Posição x do mouse usada no zoom.
    mx: f32,
    /// Alvo real do cursor; a posição usada no zoom acompanha este alvo com suavização.
    mx_target: f32,
    mouse_step: Instant,
    /// Intensidade do zoom (0..1), animada de `hover_from` até `target`.
    hover: f32,
    target: f32,
    hover_from: f32,
    hover_start: Instant,
    animating: bool,
    tracking: bool,
    /// 0 = visível, 1 = escondido abaixo da tela.
    slide: f32,
    slide_target: f32,
    slide_from: f32,
    slide_start: Instant,
    /// Janelas do dock mostradas (escondidas de vez quando slide chega a 1).
    shown: bool,
    /// Última vez que havia motivo para o dock ficar visível.
    last_inside: Option<Instant>,
    /// Arrastar ícones para reordenar/fixar/desafixar (drag.rs).
    drag: drag::Drag,
}

impl Dock {
    unsafe fn new(hwnd: HWND, glass: HWND) -> Self {
        unsafe {
            let m = Metrics::new(GetDpiForWindow(hwnd) as f32 / 96.0);
            // A barra do Windows fica escondida: o dock ocupa a base da tela.
            let (w, screen_h) = taskbar::screen_size();
            let h = m.window_height();

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .expect("CreateDIBSection");
            let memdc = CreateCompatibleDC(None);
            SelectObject(memdc, HGDIOBJ(dib.0));

            let icon_size = m.max.ceil() as i32;
            Self {
                hwnd,
                glass,
                glass_rect: None,
                glass_fx: glass::Glass::new(glass, m.radius).ok(),
                apps: apps::load_pinned(icon_size),
                stacks: stacks::Stacks::new(icon_size),
                icon_size,
                icons_mtime: icons_mtime(),
                m,
                theme: Theme::new(light_theme(), accent_color()),
                pixmap: Pixmap::new(w as u32, h as u32).expect("pixmap"),
                memdc,
                bits: bits as *mut u8,
                pos: POINT { x: 0, y: screen_h - h },
                mx: 0.0,
                mx_target: 0.0,
                mouse_step: Instant::now(),
                hover: 0.0,
                target: 0.0,
                hover_from: 0.0,
                hover_start: Instant::now(),
                animating: false,
                tracking: false,
                slide: 0.0,
                slide_target: 0.0,
                slide_from: 0.0,
                slide_start: Instant::now(),
                shown: true,
                last_inside: None,
                drag: drag::Drag::default(),
            }
        }
    }

    /// Índice do último fixado, se houver apps abertos não fixados depois dele
    /// (o botão Iniciar, na ponta direita, não conta em nenhum dos dois grupos).
    fn sep_after(&self) -> Option<usize> {
        let regular = |a: &&App| a.source != apps::START;
        let pinned = self.apps.iter().filter(regular).filter(|a| a.pinned).count();
        let open = self.apps.iter().filter(regular).filter(|a| !a.pinned).count();
        (pinned > 0 && open > 0).then(|| pinned - 1)
    }

    /// Separadores: fixados | abertos e apps | pilhas.
    fn seps(&self) -> Vec<usize> {
        self.sep_after().into_iter().chain(self.apps.len().checked_sub(1)).collect()
    }

    /// Quantos itens o dock desenha (apps e pilhas).
    fn count(&self) -> usize {
        self.apps.len() + self.stacks.items.len()
    }

    fn slots(&self) -> Vec<render::Slot> {
        let w = self.pixmap.width() as f32;
        render::layout(self.count(), &self.seps(), w, &self.m, self.mx, self.hover)
    }

    /// Pilha (source) sob o índice `i` e o ponto acima do ícone, se `i` for uma pilha.
    fn stack_at(&self, i: usize) -> Option<(String, POINT)> {
        let s = self.stacks.items.get(i.checked_sub(self.apps.len())?)?;
        Some((s.source.clone(), self.menu_anchor(i)?))
    }

    fn refresh(&mut self) -> bool {
        // Durante o arrasto a lista é do arrasto (drag.rs).
        if self.drag.busy() {
            return false;
        }
        let mtime = icons_mtime();
        let icons_changed = mtime != self.icons_mtime;
        if icons_changed {
            self.icons_mtime = mtime;
            apps::reload_icons(&mut self.apps, self.icon_size);
        }
        let stacks = self.stacks.refresh(icons_changed);
        apps::refresh(&mut self.apps, self.icon_size) || icons_changed || stacks
    }

    /// Deslocamento vertical atual da animação de esconder.
    fn offset(&self) -> i32 {
        (self.slide * self.pixmap.height() as f32).round() as i32
    }

    fn redraw(&mut self) {
        if !self.shown {
            return;
        }
        let slots = self.shown_slots();
        let seps = self.seps();
        // App em foco: dono da última janela em primeiro plano fora do dock (o menu do dock
        // toma o foco, mas o destaque continua no app).
        let fg = unsafe { GetForegroundWindow() };
        if fg != self.hwnd && fg != self.glass {
            LAST_FG.store(fg.0 as isize, Ordering::Relaxed);
        }
        let fg = HWND(LAST_FG.load(Ordering::Relaxed) as *mut c_void);
        let focus = self.apps.iter().position(|a| a.windows.contains(&fg));
        let items: Vec<&App> = self.apps.iter().chain(&self.stacks.items).collect();
        render::draw(&mut self.pixmap, &slots, &seps, &items, &self.m, &self.theme, self.hover, focus);
        self.update_glass(render::box_rect(&slots, &self.m, self.pixmap.height() as f32));

        // RGBA (tiny-skia) -> BGRA (GDI), ambos com alfa pré-multiplicado.
        let src = self.pixmap.data();
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, src.len()) };
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }

        let size = SIZE { cx: self.pixmap.width() as i32, cy: self.pixmap.height() as i32 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let pos = POINT { x: self.pos.x, y: self.pos.y + self.offset() };
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&pos),
                Some(&size),
                Some(self.memdc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    /// O DWM nem sempre re-amostra o fundo do vidro quando um app minimiza/fecha por trás
    /// (fica com a cor do app). Esconder o vidro por um quadro e mostrar de novo força a releitura.
    fn nudge_glass(&mut self) {
        if self.glass_rect.is_none() {
            return;
        }
        unsafe {
            let _ = ShowWindow(self.glass, SW_HIDE);
            let _ = DwmFlush();
        }
        if let Some(fx) = &self.glass_fx {
            fx.refresh();
        }
        self.glass_rect = None;
        self.redraw();
    }

    /// Move a janela de vidro para trás do fundo do dock.
    fn update_glass(&mut self, rect: Option<(i32, i32, i32, i32)>) {
        let off = self.offset();
        let key = rect.map(|(l, t, r, b)| (l, t + off, r, b + off));
        if key == self.glass_rect {
            return;
        }
        self.glass_rect = key;
        let Some((l, t, r, b)) = rect else {
            unsafe {
                let _ = ShowWindow(self.glass, SW_HIDE);
            }
            return;
        };
        let (w, h) = (r - l, b - t);
        if let Some(fx) = &self.glass_fx {
            fx.resize(w as f32, h as f32);
        }
        unsafe {
            let _ = SetWindowPos(
                self.glass,
                None,
                self.pos.x + l,
                self.pos.y + t + off,
                w,
                h,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
            );
        }
    }

    /// Liga o laço de animação (sincronizado com a tela em `main`).
    fn start_anim(&mut self) {
        self.animating = true;
    }

    /// Um quadro de animação, com passo proporcional ao tempo real (independe da taxa da tela).
    fn step(&mut self) {
        let now = Instant::now();

        // Suaviza os pequenos saltos dos eventos de mouse sem introduzir atraso acumulado.
        let dt = now.saturating_duration_since(self.mouse_step).as_secs_f32().min(0.05);
        self.mouse_step = now;
        let follow = 1.0 - (-dt / 0.045).exp();
        self.mx += (self.mx_target - self.mx) * follow;
        let mouse_settling = (self.mx_target - self.mx).abs() > 0.25;

        // Entrada/saída do zoom: duração fixa, recomeça do ponto atual se o alvo mudar no meio.
        let th = ((now - self.hover_start).as_secs_f32() / HOVER_TIME).min(1.0);
        self.hover = self.hover_from + (self.target - self.hover_from) * ease_out_quad(th);

        // Subir/descer: curva com duração fixa; recomeça do ponto atual se o alvo mudar no meio.
        let ease: fn(f32) -> f32 =
            if self.slide_target < self.slide_from { ease_out_cubic } else { ease_in_out_cubic };
        let t = ((now - self.slide_start).as_secs_f32() / SLIDE_TIME).min(1.0);
        self.slide = self.slide_from + (self.slide_target - self.slide_from) * ease(t);

        let settling = self.drag_step() || mouse_settling;
        if th >= 1.0 && t >= 1.0 && !attention::bouncing() && !settling {
            self.animating = false;
        }
        self.apply_shown();
        self.redraw();
    }

    /// Muda o alvo do zoom (0 ou 1) e começa a animação a partir do valor atual.
    fn set_hover(&mut self, target: f32) {
        if target != self.target {
            self.hover_from = self.hover;
            self.hover_start = Instant::now();
            self.target = target;
            self.start_anim();
        }
    }

    /// Esconde as janelas quando o dock termina de descer e mostra de novo ao subir.
    fn apply_shown(&mut self) {
        let want = self.slide < 1.0;
        if want == self.shown {
            return;
        }
        self.shown = want;
        self.glass_rect = None;
        unsafe {
            if want {
                if let Some(fx) = &self.glass_fx {
                    fx.refresh();
                }
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            } else {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                let _ = ShowWindow(self.glass, SW_HIDE);
            }
        }
    }

    /// Mouse encostado na borda de baixo da tela, na largura do dock.
    fn at_edge(&self) -> bool {
        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt) }.is_err() {
            return false;
        }
        let (_, sh) = taskbar::screen_size();
        if pt.y < sh - 2 || pt.y >= sh {
            return false;
        }
        render::box_rect(&self.slots(), &self.m, self.pixmap.height() as f32)
            .is_some_and(|(l, _, r, _)| pt.x >= self.pos.x + l && pt.x < self.pos.x + r)
    }

    /// Decide se o dock deve aparecer ou sumir (chamado a cada ~50 ms).
    fn edge_tick(&mut self) {
        let now = Instant::now();
        if self.tracking || self.drag.busy() || MENU_OPEN.load(Ordering::Relaxed) || self.at_edge() || !self.covered() {
            self.last_inside = Some(now);
        }
        // Aparecer é imediato; esconder espera a carência.
        let near = self.last_inside.is_some_and(|t| now - t < HIDE_DELAY);
        let target = if near { 0.0 } else { 1.0 };
        if target != self.slide_target {
            self.slide_from = self.slide;
            self.slide_start = now;
            self.slide_target = target;
            if target == 1.0 {
                self.set_hover(0.0);
            }
            self.start_anim();
        }
    }

    /// Alguma janela encosta no fundo do dock em repouso (sem zoom), estendido até a base da tela?
    fn covered(&self) -> bool {
        let w = self.pixmap.width() as f32;
        let slots = render::layout(self.count(), &self.seps(), w, &self.m, self.mx, 0.0);
        let Some((l, t, r, _)) = render::box_rect(&slots, &self.m, self.pixmap.height() as f32) else {
            return false;
        };
        let (_, sh) = taskbar::screen_size();
        let mut s = CoverSearch {
            rect: RECT { left: self.pos.x + l, top: self.pos.y + t, right: self.pos.x + r, bottom: sh },
            own: [self.hwnd, self.glass],
            hit: false,
        };
        unsafe {
            let _ = EnumWindows(Some(cover_proc), LPARAM(&mut s as *mut _ as isize));
        }
        s.hit
    }

    /// Ponto do menu de contexto do ícone `i`: centralizado nele, um espaço acima do topo
    /// (já com o zoom atual, que fica parado enquanto o menu está aberto).
    fn menu_anchor(&self, i: usize) -> Option<POINT> {
        let slots = self.slots();
        let slot = slots.get(i)?;
        let (_, _, _, b) = render::box_rect(&slots, &self.m, self.pixmap.height() as f32)?;
        let top = b as f32 - self.m.pad - slot.size;
        Some(POINT {
            x: self.pos.x + (slot.x + slot.size / 2.0).round() as i32,
            y: self.pos.y + self.offset() + (top - self.m.gap).round() as i32,
        })
    }

    fn hit(&self, x: f32) -> Option<usize> {
        self.slots()
            .iter()
            .position(|s| x >= s.x - self.m.gap / 2.0 && x < s.x + s.size + self.m.gap / 2.0)
    }
}

fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

/// Estado da busca de janelas que encostam no dock (passado ao `EnumWindows`).
struct CoverSearch {
    rect: RECT,
    own: [HWND; 2],
    hit: bool,
}

/// Marca `hit` e para a busca na primeira janela visível que encosta no retângulo.
unsafe extern "system" fn cover_proc(hwnd: HWND, lp: LPARAM) -> BOOL {
    unsafe {
        let s = &mut *(lp.0 as *mut CoverSearch);
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || s.own.contains(&hwnd) {
            return TRUE;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return TRUE;
        }
        // Retângulo real, sem as bordas invisíveis de redimensionar.
        let mut r = RECT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut c_void, size_of::<RECT>() as u32)
            .is_err()
        {
            return TRUE;
        }
        let d = s.rect;
        if r.right <= r.left || r.bottom <= r.top
            || r.right <= d.left || r.left >= d.right || r.bottom <= d.top || r.top >= d.bottom
        {
            return TRUE;
        }
        // Escondida pelo DWM: outra área de trabalho virtual ou app UWP suspenso.
        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut c_void, size_of::<u32>() as u32);
        if cloaked != 0 {
            return TRUE;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf).max(0) as usize;
        let class = String::from_utf16_lossy(&buf[..n]);
        // Área de trabalho e barra do Windows.
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return TRUE;
        }
        s.hit = true;
        FALSE
    }
}

fn icons_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(apps::icons_path()).and_then(|m| m.modified()).ok()
}

fn with_dock<R>(f: impl FnOnce(&mut Dock) -> R) -> Option<R> {
    // try_borrow_mut: chamadas do Windows podem reentrar no wndproc.
    DOCK.with(|d| d.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn mouse_x(lp: LPARAM) -> f32 {
    (lp.0 & 0xffff) as i16 as f32
}

fn mouse_y(lp: LPARAM) -> f32 {
    ((lp.0 >> 16) & 0xffff) as i16 as f32
}

/// Tema da barra de tarefas (claro/escuro).
/// Bandeja do sistema na topbar: os apps registram os ícones em nós (tray.rs) e a barra
/// mostra e repassa os cliques.
fn start_tray() {
    fn push() {
        // Ícone sem imagem vira um espaço vazio, para os índices baterem com os de tray.rs.
        let px = tray_px();
        let icons = tray::icons()
            .into_iter()
            .filter_map(|t| {
                let mut icon = t.icon.or_else(|| tiny_skia::Pixmap::new(px, px))?;
                grayscale(&mut icon);
                let shown = colordb::tray_shown(&t.key).unwrap_or(true);
                Some(topbar::TrayEntry { icon, tip: t.tip, key: t.key, shown })
            })
            .collect();
        topbar::set_tray(icons);
    }
    /// Tons de cinza (alfa pré-multiplicado: a média ponderada continua válida).
    fn grayscale(p: &mut tiny_skia::Pixmap) {
        for c in p.data_mut().chunks_exact_mut(4) {
            let y = (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32).round() as u8;
            c[..3].fill(y);
        }
    }
    fn clicked(index: usize, button: topbar::TrayButton, anchor: POINT) {
        let b = match button {
            topbar::TrayButton::Left => tray::TrayButton::Left,
            topbar::TrayButton::Right => tray::TrayButton::Right,
            topbar::TrayButton::Double => tray::TrayButton::Double,
        };
        tray::click(index, b, anchor);
    }
    fn tray_px() -> u32 {
        (16.0 * unsafe { GetDpiForSystem() } as f32 / 96.0).round() as u32
    }
    topbar::on_tray_click(clicked);
    if tray::start(tray_px(), push).is_ok() {
        push();
    }
}

/// Inicia com o Windows: grava o caminho deste exe em HKCU\...\Run (só se mudou).
fn autostart() {
    let Ok(exe) = std::env::current_exe() else { return };
    let value: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
    let key = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let mut buf = [0u16; 1024];
    let mut size = size_of_val(&buf) as u32;
    unsafe {
        let same = RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("ArcDockWin"),
            windows::Win32::System::Registry::RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
        .is_ok()
            && buf[..(size as usize / 2)] == value[..];
        if !same {
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key,
                w!("ArcDockWin"),
                REG_SZ.0,
                Some(value.as_ptr() as *const c_void),
                (value.len() * 2) as u32,
            );
        }
    }
}

fn light_theme() -> bool {
    let mut value: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut _ as *mut c_void),
            Some(&mut size),
        )
    };
    ok.is_ok() && value != 0
}

/// Cor de destaque do Windows (no registro vem como ABGR).
fn accent_color() -> Option<[u8; 4]> {
    let mut value: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\DWM"),
            w!("AccentColor"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut _ as *mut c_void),
            Some(&mut size),
        )
    };
    ok.is_ok().then(|| [value as u8, (value >> 8) as u8, (value >> 16) as u8, 255])
}

/// Janela em primeiro plano mudou: atualiza o destaque do app em foco na hora.
unsafe extern "system" fn on_foreground(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    with_dock(|d| {
        // O que está atrás do vidro mudou: re-amostra depois que a animação do Windows acaba.
        unsafe {
            SetTimer(Some(d.hwnd), TIMER_GLASS, 500, None);
        }
        if hwnd == d.hwnd || hwnd == d.glass {
            return;
        }
        // Janela ainda não listada (acabou de abrir): atualiza os apps antes.
        if !hwnd.is_invalid() && !d.apps.iter().any(|a| a.windows.contains(&hwnd)) {
            d.refresh();
        }
        d.redraw();
    });
}

/// Menu do botão direito: com `target` (key, fixado, janelas), as opções do app; sempre "Sair".
/// `anchor`: ponto acima do centro do ícone onde o menu se apoia (sem ícone, abre no mouse).
unsafe fn context_menu(hwnd: HWND, target: Option<(String, bool, Vec<HWND>)>, anchor: Option<POINT>) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let fg = GetForegroundWindow();
        if let Some((_, pinned, windows)) = &target {
            if windows.len() >= 2 {
                for (k, &win) in windows.iter().enumerate() {
                    let flags = if win == fg { MF_STRING | MF_CHECKED } else { MF_STRING };
                    let title = HSTRING::from(apps::menu_title(win));
                    let _ = AppendMenuW(menu, flags, MENU_WINDOW + k, &title);
                }
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            }
            let pin = HSTRING::from(crate::i18n::tr(if *pinned { "Desafixar do dock" } else { "Fixar no dock" }));
            let _ = AppendMenuW(menu, MF_STRING, MENU_PIN, &pin);
            match windows.len() {
                0 => {}
                1 => {
                    let text = HSTRING::from(crate::i18n::tr("Fechar janela"));
                    let _ = AppendMenuW(menu, MF_STRING, MENU_CLOSE, &text);
                }
                n => {
                    let text = HSTRING::from(crate::i18n::tr(&format!("Fechar {n} janelas")));
                    let _ = AppendMenuW(menu, MF_STRING, MENU_CLOSE, &text);
                }
            }
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        }
        let quit = HSTRING::from(crate::i18n::tr("Sair do ArcDockWin"));
        let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, &quit);

        let (pt, align) = match anchor {
            Some(p) => (p, TPM_CENTERALIGN),
            None => {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                (p, TPM_LEFTALIGN)
            }
        };
        let _ = SetForegroundWindow(hwnd);
        MENU_OPEN.store(true, Ordering::Relaxed);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | align,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        MENU_OPEN.store(false, Ordering::Relaxed);
        let _ = DestroyMenu(menu);

        let cmd = cmd.0 as usize;
        if cmd == MENU_QUIT {
            let _ = DestroyWindow(hwnd);
            return;
        }
        let Some((key, pinned, windows)) = target else { return };
        match cmd {
            MENU_PIN => {
                with_dock(|d| apps::set_pinned(&mut d.apps, &key, !pinned));
            }
            MENU_CLOSE => {
                for &win in &windows {
                    let _ = PostMessageW(Some(win), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            c if c >= MENU_WINDOW => {
                if let Some(&win) = windows.get(c - MENU_WINDOW) {
                    apps::focus_window(win);
                }
            }
            _ => return,
        }
        with_dock(|d| {
            d.refresh();
            d.redraw();
        });
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_LAUNCHER => {
                launcher::toggle();
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_MOUSEMOVE => {
                with_dock(|d| {
                    d.mx_target = mouse_x(lp);
                    // O ícone arrastado deve continuar colado ao cursor.
                    if d.drag.busy() {
                        d.mx = d.mx_target;
                    } else if (d.mx_target - d.mx).abs() > 0.25 {
                        d.start_anim();
                    }
                    if !d.tracking {
                        d.tracking = true;
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                    d.drag_move(d.mx_target, mouse_y(lp));
                    d.set_hover(1.0);
                    if !d.animating {
                        d.redraw();
                    }
                });
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                with_dock(|d| {
                    d.tracking = false;
                    // Com o mouse capturado no arrasto, o zoom fica até soltar.
                    if d.drag.busy() {
                        return;
                    }
                    d.set_hover(0.0);
                });
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                // A wheel scroll over a grouped app cycles through that app's windows.
                let mut point = POINT {
                    x: (lp.0 & 0xffff) as i16 as i32,
                    y: ((lp.0 >> 16) & 0xffff) as i16 as i32,
                };
                let _ = ScreenToClient(hwnd, &mut point);
                let windows = with_dock(|d| {
                    let i = d.hit(point.x as f32)?;
                    let app = d.apps.get(i)?;
                    (app.windows.len() > 1).then(|| app.windows.clone())
                })
                .flatten();
                if let Some(windows) = windows {
                    apps::cycle_window(&windows, (wp.0 >> 16) as i16 >= 0);
                    with_dock(|d| d.redraw());
                }
                LRESULT(0)
            }
            WM_TIMER => {
                with_dock(|d| match wp.0 {
                    TIMER_EDGE => d.edge_tick(),
                    TIMER_GLASS => {
                        let _ = KillTimer(Some(d.hwnd), TIMER_GLASS);
                        d.nudge_glass();
                    },
                    TIMER_REFRESH => {
                        if i18n::refresh_language() {
                            topbar::language_changed();
                            d.redraw();
                        }
                        taskbar::ensure_hidden();
                        if d.refresh() {
                            d.redraw();
                        }
                    }
                    _ => {}
                });
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                with_dock(|d| d.drag_down(mouse_x(lp), mouse_y(lp)));
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                with_dock(|d| d.drag_up());
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                // Foi um arrasto, não um clique.
                if with_dock(|d| d.drag_up()).unwrap_or(false) {
                    return LRESULT(0);
                }
                // Pilhas da ponta (Downloads, Lixeira).
                if let Some((source, anchor)) = with_dock(|d| d.stack_at(d.hit(mouse_x(lp))?)).flatten() {
                    stacks::click(&source, anchor);
                    return LRESULT(0);
                }
                let target = with_dock(|d| {
                    if d.refresh() {
                        d.redraw();
                    }
                    // get: depois do refresh o índice pode cair numa pilha.
                    d.hit(mouse_x(lp)).and_then(|i| d.apps.get(i)).map(|a| (a.source.clone(), a.windows.clone()))
                })
                .flatten();
                if let Some((source, windows)) = target {
                    apps::activate(&source, &windows);
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                if let Some((source, anchor)) = with_dock(|d| d.stack_at(d.hit(mouse_x(lp))?)).flatten() {
                    MENU_OPEN.store(true, Ordering::Relaxed);
                    stacks::context_menu(hwnd, &source, anchor);
                    MENU_OPEN.store(false, Ordering::Relaxed);
                    with_dock(|d| if d.refresh() { d.redraw() });
                    return LRESULT(0);
                }
                // App sob o mouse; fora de um ícone ou no botão Iniciar, só "Sair".
                let (target, anchor) = with_dock(|d| {
                    if d.refresh() {
                        d.redraw();
                    }
                    let i = d.hit(mouse_x(lp));
                    let target = i
                        .and_then(|i| d.apps.get(i))
                        .filter(|a| a.source != apps::START)
                        .map(|a| (a.key.clone(), a.pinned, a.windows.clone()));
                    (target, i.and_then(|i| d.menu_anchor(i)))
                })
                .unwrap_or((None, None));
                context_menu(hwnd, target, anchor);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                // Tema claro/escuro ou cor de destaque mudou: refaz as cores do dock.
                with_dock(|d| {
                    d.theme = Theme::new(light_theme(), accent_color());
                    d.redraw();
                });
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_ENDSESSION => {
                if wp.0 != 0 {
                    tray::stop();
                    topbar::shutdown();
                    taskbar::restore();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                tray::stop();
                topbar::shutdown();
                taskbar::restore();
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ if attention::is_hook(msg) => {
                if attention::handle(wp, lp) {
                    with_dock(|d| {
                        if attention::bouncing() {
                            d.start_anim();
                        }
                        d.redraw();
                    });
                }
                LRESULT(0)
            }
            _ if msg == TASKBAR_CREATED.load(Ordering::Relaxed) && msg != 0 => {
                // O Explorer reiniciou e recriou a barra: esconde de novo.
                taskbar::hide();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Menu do botão direito aberto: o dock não some enquanto ele estiver na tela.
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

/// Última janela em primeiro plano fora do dock (HWND como número).
static LAST_FG: AtomicIsize = AtomicIsize::new(0);

unsafe fn register(hinst: HINSTANCE, name: PCWSTR, proc: WNDPROC) -> windows::core::Result<()> {
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: proc,
            hInstance: hinst,
            lpszClassName: name,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        Ok(())
    }
}

unsafe extern "system" fn glass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

use windows::Win32::Foundation::HINSTANCE;

fn main() -> windows::core::Result<()> {
    if guard::run_if_requested() {
        return Ok(());
    }
    i18n::initialize();
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Uma cópia só.
        let _single = windows::Win32::System::Threading::CreateMutexW(None, true, w!("ArcDockWin.Single"))?;
        if windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            return Ok(());
        }

        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;

        // Vidro: sem superfície GDI, o conteúdo vem só da composição (glass.rs).
        let glass_class = w!("ArcDockWinGlass");
        register(hinst, glass_class, Some(glass_proc))?;
        let glass = CreateWindowExW(
            ex | WS_EX_NOREDIRECTIONBITMAP,
            glass_class,
            glass_class,
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;

        // Ícones: janela em camadas, "dona" pelo vidro para ficar sempre à frente dele.
        let class = w!("ArcDockWin");
        register(hinst, class, Some(wndproc))?;
        let hwnd = CreateWindowExW(
            ex | WS_EX_LAYERED,
            class,
            class,
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(glass),
            None,
            Some(hinst),
            None,
        )?;

        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        guard::spawn();
        attention::register(hwnd);
        taskbar::hide();

        let mut dock = Dock::new(hwnd, glass);
        dock.refresh();
        dock.redraw();
        DOCK.with(|d| *d.borrow_mut() = Some(dock));

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        topbar::create();
        start_tray();
        autostart();
        SetTimer(Some(hwnd), TIMER_REFRESH, 1000, None);
        SetTimer(Some(hwnd), TIMER_EDGE, 50, None);
        let _ = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        // Minimizar/restaurar também muda o que está atrás do vidro.
        let _ = SetWinEventHook(
            EVENT_SYSTEM_MINIMIZESTART,
            EVENT_SYSTEM_MINIMIZEEND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );

        // Parado: espera mensagens. Animando: um quadro por atualização da tela (DwmFlush
        // espera o próximo quadro do compositor), na taxa do monitor (60, 120, 144 Hz...).
        let mut msg = MSG::default();
        loop {
            if with_dock(|d| d.animating).unwrap_or(false) {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return Ok(());
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                with_dock(|d| if d.animating { d.step() });
                let _ = DwmFlush();
            } else {
                if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    Ok(())
}
