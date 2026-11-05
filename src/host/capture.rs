//! Screen capture. One implementation per OS, same interface.

use anyhow::Result;

/// A captured screen image: BGRA, 4 bytes per pixel, no row padding.
#[derive(Default)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

pub trait Capturer {
    /// Fills `out` with the current screen. Returns `false` if nothing
    /// changed since the last call (then `out` is left untouched).
    fn capture(&mut self, out: &mut Frame) -> Result<bool>;
}

pub fn open() -> Result<Box<dyn Capturer>> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
            anyhow::bail!(
                "this is a Wayland session without X11: Wayland capture is not supported yet. \
                 Log in with an \"Xorg\" session for now."
            );
        }
        Ok(Box::new(x11::X11Capturer::new()?))
    }
    #[cfg(windows)]
    {
        Ok(Box::new(dxgi::DxgiCapturer::new()?))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        anyhow::bail!("screen capture is only implemented for Linux (X11) and Windows")
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::{Capturer, Frame};
    use anyhow::{Context, Result, bail};
    use x11rb::connection::Connection;
    use x11rb::protocol::shm::ConnectionExt as _;
    use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, Window};
    use x11rb::rust_connection::RustConnection;

    /// Shared-memory segment the X server writes the screen into.
    struct Shm {
        seg: u32,
        addr: *mut u8,
        size: usize,
    }

    pub struct X11Capturer {
        conn: RustConnection,
        root: Window,
        shm: Option<Shm>,
    }

    // The raw pointer is only used from the owning thread.
    unsafe impl Send for X11Capturer {}

    impl X11Capturer {
        pub fn new() -> Result<Self> {
            let (conn, screen_num) =
                x11rb::connect(None).context("cannot connect to the X server (is DISPLAY set?)")?;
            let screen = &conn.setup().roots[screen_num];
            let root = screen.root;
            let depth = screen.root_depth;
            let bpp = conn
                .setup()
                .pixmap_formats
                .iter()
                .find(|f| f.depth == depth)
                .map(|f| f.bits_per_pixel);
            if bpp != Some(32) {
                bail!("unsupported X11 pixel format (depth {depth}, {bpp:?} bits per pixel)");
            }
            let shm_ok = conn
                .shm_query_version()
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some();
            if !shm_ok {
                tracing::warn!("X11 MIT-SHM not available, capture will be slower");
            }
            let mut c = X11Capturer {
                conn,
                root,
                shm: None,
            };
            if !shm_ok {
                c.shm = None;
                return Ok(c);
            }
            // Mark SHM as wanted by allocating a 0-size placeholder; real
            // allocation happens on first capture once the size is known.
            c.shm = Some(Shm {
                seg: 0,
                addr: std::ptr::null_mut(),
                size: 0,
            });
            Ok(c)
        }

        fn ensure_shm(&mut self, size: usize) -> Result<()> {
            let Some(shm) = &self.shm else { return Ok(()) };
            if shm.size >= size {
                return Ok(());
            }
            self.free_shm();
            unsafe {
                let id = libc::shmget(libc::IPC_PRIVATE, size, libc::IPC_CREAT | 0o600);
                if id < 0 {
                    bail!("shmget failed");
                }
                let addr = libc::shmat(id, std::ptr::null(), 0);
                if addr as isize == -1 {
                    libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
                    bail!("shmat failed");
                }
                let seg = self.conn.generate_id()?;
                let attached = self
                    .conn
                    .shm_attach(seg, id as u32, false)
                    .map(|c| c.check());
                // The segment disappears once both sides detach.
                libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
                if !matches!(attached, Ok(Ok(()))) {
                    libc::shmdt(addr);
                    bail!("X server refused the shared memory segment");
                }
                self.shm = Some(Shm {
                    seg,
                    addr: addr as *mut u8,
                    size,
                });
            }
            Ok(())
        }

        fn free_shm(&mut self) {
            if let Some(shm) = &mut self.shm
                && !shm.addr.is_null()
            {
                let _ = self.conn.shm_detach(shm.seg);
                unsafe { libc::shmdt(shm.addr as *const _) };
                shm.addr = std::ptr::null_mut();
                shm.size = 0;
            }
        }
    }

    impl Drop for X11Capturer {
        fn drop(&mut self) {
            self.free_shm();
        }
    }

    impl Capturer for X11Capturer {
        fn capture(&mut self, out: &mut Frame) -> Result<bool> {
            let geo = self.conn.get_geometry(self.root)?.reply()?;
            let (w, h) = (geo.width as usize, geo.height as usize);
            let size = w * h * 4;
            if self.shm.is_some()
                && let Err(e) = self.ensure_shm(size)
            {
                tracing::warn!("MIT-SHM setup failed ({e}), falling back to plain capture");
                self.shm = None;
            }
            out.width = w;
            out.height = h;
            out.bgra.resize(size, 0);
            match &self.shm {
                Some(shm) => {
                    self.conn
                        .shm_get_image(
                            self.root,
                            0,
                            0,
                            w as u16,
                            h as u16,
                            !0,
                            ImageFormat::Z_PIXMAP.into(),
                            shm.seg,
                            0,
                        )?
                        .reply()?;
                    let src = unsafe { std::slice::from_raw_parts(shm.addr, size) };
                    out.bgra.copy_from_slice(src);
                }
                None => {
                    let img = self
                        .conn
                        .get_image(
                            ImageFormat::Z_PIXMAP,
                            self.root,
                            0,
                            0,
                            w as u16,
                            h as u16,
                            !0,
                        )?
                        .reply()?;
                    out.bgra.copy_from_slice(&img.data[..size]);
                }
            }
            Ok(true)
        }
    }
}

#[cfg(windows)]
mod dxgi {
    //! DXGI Desktop Duplication: the GPU hands us the desktop image of the
    //! primary monitor whenever it changes.

    use super::{Capturer, Frame};
    use anyhow::{Context, Result};
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::*;
    use windows::core::Interface;

    pub struct DxgiCapturer {
        device: ID3D11Device,
        ctx: ID3D11DeviceContext,
        dup: IDXGIOutputDuplication,
        staging: Option<(ID3D11Texture2D, u32, u32)>,
        have_frame: bool,
    }

    // COM objects are used only from the capture thread.
    unsafe impl Send for DxgiCapturer {}

    fn duplicate(device: &ID3D11Device) -> Result<IDXGIOutputDuplication> {
        unsafe {
            let dxgi: IDXGIDevice = device.cast()?;
            let adapter = dxgi.GetAdapter()?;
            let output = adapter.EnumOutputs(0).context("no monitor found")?;
            let output1: IDXGIOutput1 = output.cast()?;
            output1
                .DuplicateOutput(device)
                .context("DuplicateOutput failed")
        }
    }

    impl DxgiCapturer {
        pub fn new() -> Result<Self> {
            unsafe {
                let mut device = None;
                let mut ctx = None;
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut ctx),
                )
                .context("cannot create a Direct3D 11 device")?;
                let device = device.context("no D3D11 device")?;
                let ctx = ctx.context("no D3D11 context")?;
                let dup = duplicate(&device)?;
                Ok(DxgiCapturer {
                    device,
                    ctx,
                    dup,
                    staging: None,
                    have_frame: false,
                })
            }
        }
    }

    impl Capturer for DxgiCapturer {
        fn capture(&mut self, out: &mut Frame) -> Result<bool> {
            unsafe {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut res: Option<IDXGIResource> = None;
                // Wait a little for a change; after the first frame, "no
                // change" is a normal answer.
                let timeout = if self.have_frame { 16 } else { 500 };
                if let Err(e) = self.dup.AcquireNextFrame(timeout, &mut info, &mut res) {
                    if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                        return Ok(false);
                    }
                    if e.code() == DXGI_ERROR_ACCESS_LOST {
                        // Mode change, UAC prompt, lock screen...: start over.
                        self.dup = duplicate(&self.device)?;
                        self.staging = None;
                        return Ok(false);
                    }
                    return Err(e.into());
                }
                let result = (|| -> Result<bool> {
                    // Only the mouse moved: the image itself is unchanged.
                    if info.LastPresentTime == 0 && self.have_frame {
                        return Ok(false);
                    }
                    let tex: ID3D11Texture2D = res.as_ref().context("no desktop image")?.cast()?;
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    tex.GetDesc(&mut desc);
                    let need_new = !matches!(&self.staging, Some((_, w, h)) if *w == desc.Width && *h == desc.Height);
                    if need_new {
                        let mut sd = desc;
                        sd.Usage = D3D11_USAGE_STAGING;
                        sd.BindFlags = 0;
                        sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                        sd.MiscFlags = 0;
                        sd.MipLevels = 1;
                        sd.ArraySize = 1;
                        let mut staging = None;
                        self.device.CreateTexture2D(&sd, None, Some(&mut staging))?;
                        self.staging = Some((
                            staging.context("no staging texture")?,
                            desc.Width,
                            desc.Height,
                        ));
                    }
                    let (staging, w, h) = self.staging.as_ref().unwrap();
                    let (w, h) = (*w as usize, *h as usize);
                    self.ctx.CopyResource(staging, &tex);
                    let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                    self.ctx
                        .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
                    out.width = w;
                    out.height = h;
                    out.bgra.resize(w * h * 4, 0);
                    let pitch = map.RowPitch as usize;
                    let src = std::slice::from_raw_parts(map.pData as *const u8, pitch * h);
                    for y in 0..h {
                        out.bgra[y * w * 4..(y + 1) * w * 4]
                            .copy_from_slice(&src[y * pitch..y * pitch + w * 4]);
                    }
                    self.ctx.Unmap(staging, 0);
                    self.have_frame = true;
                    Ok(true)
                })();
                let _ = self.dup.ReleaseFrame();
                result
            }
        }
    }
}
