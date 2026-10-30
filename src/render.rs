//! The video chip's picture, drawn on the GPU, in a window that also takes
//! the keyboard for the input chip (gamepad.rs reads the gamepads).
//!
//! Each vblank where the picture changed, the bus hands over a copy of VRAM
//! and the video registers. VRAM goes up as a texture of bytes, and a
//! fragment shader works out every pixel the way the chip does (vdp.rs,
//! docs/machine.md): background tiles, then sprites. The 320x240 result is
//! scaled into the window.

use crate::backlight::Blanker;
use crate::input::{self, Input};
use crate::status::Status;
use crate::vdp::{HEIGHT, WIDTH};
use glium::glutin::config::{Api, ConfigTemplateBuilder, GlConfig};
use glium::glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext, Version};
use glium::glutin::display::{GetGlDisplay, GlDisplay};
use glium::glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glium::texture::{ClientFormat, MipmapsOption, RawImage2d, UncompressedFloatFormat};
use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter};
use glium::winit::dpi::PhysicalSize;
use glium::winit::event::{ElementState, WindowEvent};
use glium::winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use glium::winit::keyboard::{Key, NamedKey};
use glium::winit::raw_window_handle::HasWindowHandle;
use glium::winit::window::{Fullscreen, Window};
use glium::{Surface, implement_vertex, program};
use glutin_winit::DisplayBuilder;
use std::borrow::Cow;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How often the window checks on the screen blanker. Frames don't wait for
/// it: one wakes the window as it comes.
const TICK: Duration = Duration::from_secs(1);

/// One vblank's copy of what the video chip shows.
pub struct Frame {
    pub vram: Vec<u8>,
    pub ctrl: u8,
    /// Each of the 240 lines' scroll, plane A's and then plane B's: X then
    /// Y, 16 bits each, little-endian.
    pub lines: Vec<u8>,
    pub backdrop: u8,
}

#[cfg(test)]
impl Frame {
    /// Line `line`'s scroll, X and Y.
    pub fn scroll(&self, line: usize) -> (u16, u16) {
        let at = &self.lines[line * 4..line * 4 + 4];
        (u16::from_le_bytes([at[0], at[1]]), u16::from_le_bytes([at[2], at[3]]))
    }

    /// Line `line`'s scroll for plane B.
    pub fn scroll_b(&self, line: usize) -> (u16, u16) {
        let at = &self.lines[240 * 4 + line * 4..240 * 4 + line * 4 + 4];
        (u16::from_le_bytes([at[0], at[1]]), u16::from_le_bytes([at[2], at[3]]))
    }
}

/// The newest frame, handed from the CPU thread to the window. The CPU checks
/// `empty` without locking and only offers a frame once the window has taken
/// the last one; an offer wakes the window to draw it at once.
pub struct FrameSlot {
    frame: Mutex<Option<Frame>>,
    empty: AtomicBool,
    wake: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl Default for FrameSlot {
    fn default() -> Self {
        FrameSlot {
            frame: Mutex::new(None),
            empty: AtomicBool::new(true),
            wake: OnceLock::new(),
        }
    }
}

impl FrameSlot {
    pub fn is_empty(&self) -> bool {
        self.empty.load(Ordering::Acquire)
    }

    pub fn offer(&self, frame: Frame) {
        *self.frame.lock().unwrap() = Some(frame);
        self.empty.store(false, Ordering::Release);
        if let Some(wake) = self.wake.get() {
            wake();
        }
    }

    pub(crate) fn take(&self) -> Option<Frame> {
        if self.is_empty() {
            return None;
        }
        let frame = self.frame.lock().unwrap().take();
        self.empty.store(true, Ordering::Release);
        frame
    }
}

/// The pad button a key stands for while held (KEYPAD), as SNES emulators
/// lay them out: the arrows, Z for B, X for A, A for Y, S for X, Q and W
/// for L1 and R1, Return for Start, right Shift for Select. Keys by
/// position, whatever the layout.
fn keypad_button(event: &glium::winit::event::KeyEvent) -> u16 {
    use glium::winit::keyboard::{KeyCode, PhysicalKey};
    let PhysicalKey::Code(code) = event.physical_key else { return 0 };
    match code {
        KeyCode::ArrowUp => input::PAD_UP,
        KeyCode::ArrowDown => input::PAD_DOWN,
        KeyCode::ArrowLeft => input::PAD_LEFT,
        KeyCode::ArrowRight => input::PAD_RIGHT,
        KeyCode::KeyZ => input::PAD_B,
        KeyCode::KeyX => input::PAD_A,
        KeyCode::KeyA => input::PAD_Y,
        KeyCode::KeyS => input::PAD_X,
        KeyCode::KeyQ => input::PAD_L1,
        KeyCode::KeyW => input::PAD_R1,
        KeyCode::Enter | KeyCode::NumpadEnter => input::PAD_START,
        KeyCode::ShiftRight => input::PAD_SELECT,
        _ => 0,
    }
}

/// The key code a key press sends to the input chip, if any.
fn key_code(event: &glium::winit::event::KeyEvent) -> Option<u8> {
    match &event.logical_key {
        Key::Named(NamedKey::Enter) => Some(b'\r'),
        Key::Named(NamedKey::Backspace) => Some(0x08),
        Key::Named(NamedKey::Escape) => Some(0x1B),
        Key::Named(NamedKey::Tab) => Some(b'\t'),
        Key::Named(NamedKey::ArrowUp) => Some(input::KEY_UP),
        Key::Named(NamedKey::ArrowDown) => Some(input::KEY_DOWN),
        Key::Named(NamedKey::ArrowLeft) => Some(input::KEY_LEFT),
        Key::Named(NamedKey::ArrowRight) => Some(input::KEY_RIGHT),
        _ => {
            let text = event.text.as_ref()?;
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) if ch.is_ascii() => Some(ch as u8),
                _ => None,
            }
        }
    }
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

implement_vertex!(Vertex, position, tex_coords);

/// The video chip, as a fragment shader over a 320x240 target. VRAM is a
/// 256x256 texture of bytes, byte `a` at (a & 255, a >> 8); the sprite table
/// is a 64x1 texture of 4-byte entries. They are 8-bit normalized textures,
/// which every GL and GLES 3 driver takes, read back into bytes here.
const CHIP_SHADER: &str = "
    uniform highp sampler2D vram;
    uniform highp sampler2D sprites;
    uniform highp sampler2D lines;
    uniform int ctrl;
    uniform int backdrop;
    out vec4 color;

    int peek(int addr) {
        return int(texelFetch(vram, ivec2(addr & 255, (addr >> 8) & 255), 0).r * 255.0 + 0.5);
    }

    int word(int addr) {
        return peek(addr) | peek(addr + 1) << 8;
    }

    vec4 rgb(int palette, int index) {
        int c = word(0x9100 + (palette * 16 + index) * 2);
        return vec4(float(c >> 8 & 15), float(c >> 4 & 15), float(c & 15), 15.0) / 15.0;
    }

    int tile_pixel(int tile, int x, int y) {
        int b = peek((tile * 32 + y * 4 + x / 2) & 0xFFFF);
        return (x & 1) == 0 ? b >> 4 : b & 15;
    }

    // A plane's colour number at (px, py) of its map, and its palette.
    int plane(int map, int px, int py, out int palette) {
        int mx = px & 511;
        int my = py & 255;
        int entry = word(map + ((my >> 3) * 64 + (mx >> 3)) * 2);
        int tx = (entry & 0x400) != 0 ? 7 - (mx & 7) : mx & 7;
        int ty = (entry & 0x800) != 0 ? 7 - (my & 7) : my & 7;
        palette = entry >> 12;
        return tile_pixel(entry & 0x3FF, tx, ty);
    }

    void main() {
        int x = int(gl_FragCoord.x);
        int y = int(gl_FragCoord.y);
        if ((ctrl & 1) == 0) {
            color = vec4(0.0, 0.0, 0.0, 1.0);
            return;
        }
        // The scrolls this line had as the beam passed it: row 0 plane A's,
        // row 1 plane B's.
        ivec4 sa = ivec4(texelFetch(lines, ivec2(y, 0), 0) * 255.0 + 0.5);
        int pa;
        int a = plane(0x8000, x + (sa.r | sa.g << 8), y + (sa.b | sa.a << 8), pa);
        int pb = 0;
        int b = 0;
        if ((ctrl & 8) != 0) {
            ivec4 sb = ivec4(texelFetch(lines, ivec2(y, 1), 0) * 255.0 + 0.5);
            b = plane(0xA000, x + (sb.r | sb.g << 8), y + sb.b, pb);
        }
        // The frontmost opaque sprite (sprite 0 on top), and its priority.
        int sc = 0;
        int sp = 0;
        int priority = 0;
        if ((ctrl & 2) != 0) {
            int size = (ctrl & 4) != 0 ? 16 : 8;
            for (int s = 0; s < 64; s++) {
                ivec4 e = ivec4(texelFetch(sprites, ivec2(s, 0), 0) * 255.0 + 0.5);
                int top = e.r;
                int attributes = e.a;
                // X wraps: 496-511 are -16 to -1, partly off the left edge.
                int lx = (x - (e.g | (attributes & 1) << 8)) & 511;
                int ly = y - top;
                if (top >= 240 || lx < 0 || ly < 0 || lx >= size || ly >= size) {
                    continue;
                }
                if ((attributes & 4) != 0) lx = size - 1 - lx;
                if ((attributes & 8) != 0) ly = size - 1 - ly;
                int tile = (e.b | (attributes & 2) << 7) + (ly >> 3) * 2 + (lx >> 3);
                int c = tile_pixel(tile, lx & 7, ly & 7);
                if (c != 0) {
                    sc = c;
                    sp = attributes >> 4;
                    priority = peek(0x9300 + s) & 3;
                    break;
                }
            }
        }
        // Back to front: backdrop, sprites behind both planes, plane A,
        // sprites behind B, plane B, sprites in front.
        color = rgb(0, backdrop);
        if (sc != 0 && priority >= 2) color = rgb(sp, sc);
        if (a != 0) color = rgb(pa, a);
        if (sc != 0 && priority == 1) color = rgb(sp, sc);
        if (b != 0) color = rgb(pb, b);
        if (sc != 0 && priority == 0) color = rgb(sp, sc);
    }
";

const QUAD_VERTEX: &str = "
    in vec2 position;
    in vec2 tex_coords;
    out vec2 v_tex_coords;

    void main() {
        v_tex_coords = tex_coords;
        gl_Position = vec4(position, 0.0, 1.0);
    }
";

const PRESENT_SHADER: &str = "
    in vec2 v_tex_coords;
    out vec4 color;
    uniform sampler2D picture;

    void main() {
        color = texture(picture, v_tex_coords);
    }
";

struct App {
    window: Window,
    display: glium::Display<WindowSurface>,
    quad: glium::VertexBuffer<Vertex>,
    chip: glium::Program,
    present: glium::Program,
    /// The chip's output, 320x240.
    picture: glium::Texture2d,
    backdrop: [f32; 3],
    frames: Arc<FrameSlot>,
    input: Arc<Input>,
    status: Arc<Status>,
    /// Select + Start were both held at the last poll.
    clicked: Option<Instant>,
    /// None without `--blank-after`.
    blanker: Option<Arc<Mutex<Blanker>>>,
    next_tick: Instant,
}

impl App {
    /// Once per tick: blank the screen if it is time to, and draw a frame
    /// that came without waking the window (one offered before the window
    /// could be woken).
    fn tick(&mut self) {
        if let Some(blanker) = &self.blanker {
            blanker.lock().unwrap().tick(self.status.on_battery());
        }
        self.new_frame();
    }

    /// A new frame came: run the chip over it and show it. An unchanged
    /// picture costs nothing, as no frame comes.
    fn new_frame(&mut self) {
        if let Some(frame) = self.frames.take() {
            self.draw_chip(&frame);
            self.window.request_redraw();
        }
    }

    fn draw_chip(&mut self, frame: &Frame) {
        let upload = |data: &[u8], width: u32, client: ClientFormat, format: UncompressedFloatFormat| {
            let height = data.len() as u32 / width / client.get_size() as u32;
            let image = RawImage2d { data: Cow::Borrowed(data), width, height, format: client };
            glium::Texture2d::with_format(&self.display, image, format, MipmapsOption::NoMipmap)
                .expect("VRAM texture")
        };
        let vram = upload(&frame.vram, 256, ClientFormat::U8, UncompressedFloatFormat::U8);
        let sprites = upload(
            &frame.vram[0x9000..0x9100],
            64,
            ClientFormat::U8U8U8U8,
            UncompressedFloatFormat::U8U8U8U8,
        );
        let lines = upload(&frame.lines, 240, ClientFormat::U8U8U8U8, UncompressedFloatFormat::U8U8U8U8);
        let at = 0x9100 + frame.backdrop as usize * 2;
        let c = u16::from_le_bytes([frame.vram[at], frame.vram[at + 1]]);
        self.backdrop = [8, 4, 0].map(|shift| (c >> shift & 15) as f32 / 15.0);

        let uniforms = glium::uniform! {
            vram: vram.sampled()
                .magnify_filter(MagnifySamplerFilter::Nearest)
                .minify_filter(MinifySamplerFilter::Nearest),
            sprites: sprites.sampled()
                .magnify_filter(MagnifySamplerFilter::Nearest)
                .minify_filter(MinifySamplerFilter::Nearest),
            lines: lines.sampled()
                .magnify_filter(MagnifySamplerFilter::Nearest)
                .minify_filter(MinifySamplerFilter::Nearest),
            ctrl: frame.ctrl as i32,
            backdrop: frame.backdrop as i32,
        };
        let mut target = glium::framebuffer::SimpleFrameBuffer::new(&self.display, &self.picture)
            .expect("picture framebuffer");
        let indices = glium::index::NoIndices(glium::index::PrimitiveType::TriangleStrip);
        target
            .draw(&self.quad, indices, &self.chip, &uniforms, &Default::default())
            .expect("video chip");
    }

    /// Show the picture as large as fits at 4:3, the rest in the backdrop
    /// colour.
    fn present(&self) {
        let (width, height) = self.display.get_framebuffer_dimensions();
        let scale = (width as f32 / WIDTH as f32).min(height as f32 / HEIGHT as f32);
        let (w, h) = ((WIDTH as f32 * scale) as u32, (HEIGHT as f32 * scale) as u32);
        let viewport = glium::Rect {
            left: (width - w) / 2,
            bottom: (height - h) / 2,
            width: w,
            height: h,
        };
        let mut target = self.display.draw();
        let [r, g, b] = self.backdrop;
        target.clear_color(r, g, b, 1.0);
        let uniforms = glium::uniform! {
            picture: self.picture.sampled()
                .magnify_filter(MagnifySamplerFilter::Nearest)
                .minify_filter(MinifySamplerFilter::Nearest),
        };
        let parameters = glium::DrawParameters {
            viewport: Some(viewport),
            ..Default::default()
        };
        let indices = glium::index::NoIndices(glium::index::PrimitiveType::TriangleStrip);
        target
            .draw(&self.quad, indices, &self.present, &uniforms, &parameters)
            .expect("present");
        target.finish().expect("present");
    }

    fn fullscreen(&self) {
        let fullscreen = match self.window.fullscreen() {
            Some(_) => None,
            None => Some(Fullscreen::Borderless(None)),
        };
        self.window.set_fullscreen(fullscreen);
    }

}

impl glium::winit::application::ApplicationHandler<()> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        self.new_frame();
        self.window.request_redraw();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _frame: ()) {
        self.new_frame();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_tick {
            self.next_tick = now + TICK;
            self.tick();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: glium::winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => std::process::exit(0),
            WindowEvent::Resized(size) => {
                self.display.resize(size.into());
                self.window.request_redraw();
            }
            WindowEvent::Focused(false) => self.input.keypad.store(0, Ordering::Relaxed),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Released => {
                self.input.keypad.fetch_and(!keypad_button(&event), Ordering::Relaxed);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.input.keypad.fetch_or(keypad_button(&event), Ordering::Relaxed);
                if self.blanker.as_ref().is_some_and(|b| b.lock().unwrap().input()) {
                    return;
                }
                match event.logical_key {
                    Key::Named(NamedKey::F11) => self.fullscreen(),
                    Key::Named(NamedKey::F12) => self.input.reset.store(true, Ordering::Release),
                    _ => {
                        if let Some(code) = key_code(&event) {
                            self.input.press_key(code);
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: glium::winit::event::MouseButton::Left,
                ..
            } => match self.clicked.take() {
                Some(clicked) if clicked.elapsed() < Duration::from_millis(500) => self.fullscreen(),
                _ => self.clicked = Some(Instant::now()),
            },
            WindowEvent::RedrawRequested => self.present(),
            _ => {}
        }
    }
}

/// Open the window and run it until it is closed. Must be called on the main
/// thread; the CPU runs on its own.
pub fn run(
    input: Arc<Input>,
    frames: Arc<FrameSlot>,
    status: Arc<Status>,
    fullscreen: bool,
    blanker: Option<Arc<Mutex<Blanker>>>,
) {
    let event_loop = EventLoop::builder().build().expect("event loop building");
    let proxy = event_loop.create_proxy();
    let _ = frames.wake.set(Box::new(move || {
        let _ = proxy.send_event(());
    }));
    let (window, display) = create_display(&event_loop, fullscreen);

    // Texture row 0 is the top of the picture.
    let quad = glium::VertexBuffer::new(
        &display,
        &[
            Vertex { position: [-1.0, 1.0], tex_coords: [0.0, 0.0] },
            Vertex { position: [1.0, 1.0], tex_coords: [1.0, 0.0] },
            Vertex { position: [-1.0, -1.0], tex_coords: [0.0, 1.0] },
            Vertex { position: [1.0, -1.0], tex_coords: [1.0, 1.0] },
        ],
    )
    .expect("quad");

    // GLSL 1.40 for desktop OpenGL, GLSL ES 3.00 for OpenGL ES; glium picks
    // whichever the context supports.
    let desktop = |body: &str| format!("#version 140\n{body}");
    let gles = |body: &str| format!("#version 300 es\nprecision highp float;\nprecision highp int;\n{body}");
    let chip = program!(&display,
        140 => { vertex: &desktop(QUAD_VERTEX), fragment: &desktop(CHIP_SHADER) },
        300 es => { vertex: &gles(QUAD_VERTEX), fragment: &gles(CHIP_SHADER) },
    )
    .expect("video chip shader");
    let present = program!(&display,
        140 => { vertex: &desktop(QUAD_VERTEX), fragment: &desktop(PRESENT_SHADER) },
        300 es => { vertex: &gles(QUAD_VERTEX), fragment: &gles(PRESENT_SHADER) },
    )
    .expect("present shader");
    let picture = glium::Texture2d::empty(&display, WIDTH as u32, HEIGHT as u32).expect("picture");

    let mut app = App {
        window,
        display,
        quad,
        chip,
        present,
        picture,
        backdrop: [0.0; 3],
        frames,
        input,
        status,
        clicked: None,
        blanker,
        next_tick: Instant::now(),
    };
    event_loop.run_app(&mut app).expect("event loop error");
}

/// The window and its GL display: desktop OpenGL where the driver offers it,
/// otherwise OpenGL ES 3.0 (Mesa panfrost on the RG351MP, for one). Based on
/// glium's `SimpleWindowBuilder::build`, which takes the first config and
/// only asks for desktop GL.
fn create_display(event_loop: &EventLoop<()>, fullscreen: bool) -> (Window, glium::Display<WindowSurface>) {
    let attributes = Window::default_attributes()
        .with_title("6502 Console")
        .with_inner_size(PhysicalSize::new(2 * WIDTH as u32, 2 * HEIGHT as u32))
        .with_decorations(!fullscreen)
        .with_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
    let (window, config) = DisplayBuilder::new()
        .with_window_attributes(Some(attributes))
        .build(event_loop, ConfigTemplateBuilder::new(), |configs| {
            let configs: Vec<_> = configs.collect();
            let desktop = configs.iter().find(|c| c.api().contains(Api::OPENGL));
            desktop.or(configs.first()).cloned().expect("no GL config")
        })
        .expect("display building");
    let window = window.expect("window");
    let handle = window.window_handle().expect("window handle").as_raw();
    let gl_display = config.display();

    let desktop = ContextAttributesBuilder::new().build(Some(handle));
    let gles = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
        .build(Some(handle));
    let context = unsafe { gl_display.create_context(&config, &desktop) }
        .or_else(|_| unsafe { gl_display.create_context(&config, &gles) })
        .expect("failed to create an OpenGL or OpenGL ES context");

    let (width, height): (u32, u32) = window.inner_size().into();
    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        handle,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    );
    let surface = unsafe { gl_display.create_window_surface(&config, &surface_attributes) }
        .expect("window surface");
    let context = context.make_current(&surface).expect("make context current");
    let display = glium::Display::from_context_surface(context, surface).expect("glium display");
    (window, display)
}
