use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalKind {
    FilePicker,
    AudioOutput,
    Notifications,
    Clipboard,
    Screenshot,
    Settings,
}

impl PortalKind {
    pub fn name(&self) -> &'static str {
        match self {
            PortalKind::FilePicker => "org.eos.portal.FilePicker",
            PortalKind::AudioOutput => "org.eos.portal.AudioOutput",
            PortalKind::Notifications => "org.eos.portal.Notifications",
            PortalKind::Clipboard => "org.eos.portal.Clipboard",
            PortalKind::Screenshot => "org.eos.portal.Screenshot",
            PortalKind::Settings => "org.eos.portal.Settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalAccess {
    /// مصدر حصري: يُمنح لتطبيق واحد فقط (مثل FilePicker لـ Finder)
    Exclusive,
    /// غير محدود: متاح لكل التطبيقات كمزود أو مستهلك (مثل Audio و Notifications)
    Unlimited,
}

#[derive(Clone, Debug)]
pub struct PortalEntry {
    pub kind: PortalKind,
    pub access: PortalAccess,
    pub providers: Vec<String>,
    pub request_count: u64,
}

#[derive(Clone, Debug)]
pub struct PortalRequest {
    pub kind: PortalKind,
    pub caller: String,
    pub payload: String,
}

#[derive(Clone, Debug)]
pub struct ToastNotification {
    pub title: String,
    pub message: String,
    pub created_at: std::time::Instant,
    pub duration_ms: u64,
}

struct PortalRegistry {
    entries: Vec<PortalEntry>,
    last_log: String,
    pending_file_picker_caller: Option<(String, String)>, // (caller, allowed_exts)
    completed_file_pick: Option<(String, String)>, // (caller, selected_path)
    toasts: Vec<ToastNotification>,
}

static PORTAL_REG: Mutex<Option<PortalRegistry>> = Mutex::new(None);
static ISOLATED_WIN_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

pub fn set_isolated_win_id(id: u32) {
    ISOLATED_WIN_ID.store(id, core::sync::atomic::Ordering::Relaxed);
}

pub fn request_fullscreen_toggle() {
    let ext_id = ISOLATED_WIN_ID.load(core::sync::atomic::Ordering::Relaxed);
    if ext_id != 0 {
        crate::syscall::sys_win_file_portal(3, ext_id, &mut []);
    }
}

fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut PortalRegistry) -> R,
{
    let mut lock = PORTAL_REG.lock().unwrap();
    if lock.is_none() {
        *lock = Some(PortalRegistry {
            entries: vec![
                PortalEntry { kind: PortalKind::FilePicker, access: PortalAccess::Exclusive, providers: Vec::new(), request_count: 0 },
                PortalEntry { kind: PortalKind::Settings, access: PortalAccess::Exclusive, providers: Vec::new(), request_count: 0 },
                PortalEntry { kind: PortalKind::Screenshot, access: PortalAccess::Exclusive, providers: Vec::new(), request_count: 0 },
                PortalEntry { kind: PortalKind::AudioOutput, access: PortalAccess::Unlimited, providers: Vec::new(), request_count: 0 },
                PortalEntry { kind: PortalKind::Notifications, access: PortalAccess::Unlimited, providers: Vec::new(), request_count: 0 },
                PortalEntry { kind: PortalKind::Clipboard, access: PortalAccess::Unlimited, providers: Vec::new(), request_count: 0 },
            ],
            last_log: "EOS Portal Bus Initialized".into(),
            pending_file_picker_caller: None,
            completed_file_pick: None,
            toasts: Vec::new(),
        });
    }
    f(lock.as_mut().unwrap())
}

/// تسجيل تطبيق كمصدر (Provider) لبوابة معينة.
/// إذا كانت البوابة Exclusive ولديها مصدر مسجّل مسبقاً، يُرفض الطلب للحفاظ على الحصرية.
pub fn register_portal_provider(kind: PortalKind, provider_app: &str) -> Result<(), String> {
    with_registry(|reg| {
        if let Some(entry) = reg.entries.iter_mut().find(|e| e.kind == kind) {
            if entry.access == PortalAccess::Exclusive && !entry.providers.is_empty() {
                let owner = entry.providers[0].clone();
                if owner == provider_app {
                    return Ok(());
                }
                let msg = format!("DENIED: {} is Exclusive to '{}'", kind.name(), owner);
                reg.last_log = msg.clone();
                return Err(msg);
            }
            if !entry.providers.iter().any(|p| p == provider_app) {
                entry.providers.push(provider_app.to_string());
            }
            reg.last_log = format!("BOUND: {} -> {}", kind.name(), provider_app);
            return Ok(());
        }
        Err("Unknown PortalKind".into())
    })
}

/// إرسال طلب من أي تطبيق عبر البوابة ليُوجّه تلقائياً إلى المصدر المعتمد
pub fn request_portal(req: PortalRequest) -> Result<String, String> {
    with_registry(|reg| {
        if let Some(entry) = reg.entries.iter_mut().find(|e| e.kind == req.kind) {
            entry.request_count += 1;
            let provider = entry.providers.first().cloned().unwrap_or_else(|| "Unbound".into());
            if req.kind == PortalKind::FilePicker {
                let ext_id = ISOLATED_WIN_ID.load(core::sync::atomic::Ordering::Relaxed);
                if ext_id != 0 {
                    let mut filter_bytes = req.payload.clone().into_bytes();
                    crate::syscall::sys_win_file_portal(0, ext_id, &mut filter_bytes);
                } else {
                    reg.pending_file_picker_caller = Some((req.caller.clone(), req.payload.clone()));
                }
            } else if req.kind == PortalKind::Notifications {
                let (title, body) = match req.payload.split_once('|') {
                    Some((t, b)) => (t.trim().to_string(), b.trim().to_string()),
                    None => (req.caller.clone(), req.payload.clone()),
                };
                if reg.toasts.len() >= 4 {
                    reg.toasts.remove(0);
                }
                reg.toasts.push(ToastNotification {
                    title,
                    message: body,
                    created_at: std::time::Instant::now(),
                    duration_ms: 4000,
                });
            }
            let msg = format!(
                "ROUTED [{}]: '{}' -> Provider '{}' (payload: '{}')",
                req.kind.name(),
                req.caller,
                provider,
                req.payload
            );
            reg.last_log = msg.clone();
            return Ok(msg);
        }
        Err("Portal not registered".into())
    })
}

pub fn list_portals() -> Vec<PortalEntry> {
    with_registry(|reg| reg.entries.clone())
}

pub fn last_portal_log() -> String {
    with_registry(|reg| reg.last_log.clone())
}

pub fn take_pending_file_picker() -> Option<(String, String)> {
    with_registry(|reg| reg.pending_file_picker_caller.take())
}

pub fn request_always_on_top_toggle() {
    let ext_id = ISOLATED_WIN_ID.load(core::sync::atomic::Ordering::Relaxed);
    if ext_id != 0 {
        crate::syscall::sys_win_file_portal(5, ext_id, &mut []);
    }
}

pub fn complete_file_picker(caller: String, path: String) {
    with_registry(|reg| {
        reg.last_log = format!("PICKED [FilePicker]: '{}' -> {}", caller, path);
        reg.completed_file_pick = Some((caller, path));
    });
}

pub fn take_completed_file_pick(caller: &str) -> Option<String> {
    let ext_id = ISOLATED_WIN_ID.load(core::sync::atomic::Ordering::Relaxed);
    if ext_id != 0 {
        let mut buf = [0u8; 128];
        let n = crate::syscall::sys_win_file_portal(2, ext_id, &mut buf);
        if n > 0 {
            return Some(String::from_utf8_lossy(&buf[..n]).to_string());
        }
        return None;
    }
    with_registry(|reg| {
        if let Some((ref c, _)) = reg.completed_file_pick {
            if c == caller {
                return reg.completed_file_pick.take().map(|(_, p)| p);
            }
        }
        None
    })
}

pub fn push_toast(title: &str, message: &str) {
    let _ = request_portal(PortalRequest {
        kind: PortalKind::Notifications,
        caller: title.to_string(),
        payload: format!("{}|{}", title, message),
    });
}

pub fn poll_active_toasts() -> Vec<ToastNotification> {
    with_registry(|reg| {
        let now = std::time::Instant::now();
        reg.toasts.retain(|t| now.saturating_duration_since(t.created_at).as_millis() < t.duration_ms as u128);
        reg.toasts.clone()
    })
}

pub fn init_default_portals() {
    let _ = register_portal_provider(PortalKind::FilePicker, "Finder");
    let _ = register_portal_provider(PortalKind::Settings, "Settings");
    let _ = register_portal_provider(PortalKind::Screenshot, "EOS Shell");
    let _ = register_portal_provider(PortalKind::AudioOutput, "EOS Audio");
    let _ = register_portal_provider(PortalKind::Notifications, "EOS Shell");
    let _ = register_portal_provider(PortalKind::Clipboard, "EOS Clipboard");
}
