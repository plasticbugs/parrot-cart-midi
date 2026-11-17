//! The emulated MT-32: Munt's libmt32emu through its C interface
//! (vendor/munt/mt32emu/src/c_interface/c_interface.h), bound by hand for the
//! few functions used here, and the search for the ROMs it needs.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::mpsc::Sender;

use crate::synth::Synth;

// ---- the C interface ---------------------------------------------------------------

#[repr(C)]
struct Data {
    _opaque: [u8; 0],
}
type Context = *mut Data;

#[repr(C)]
struct RomInfo {
    control_rom_id: *const c_char,
    control_rom_description: *const c_char,
    control_rom_sha1_digest: *const c_char,
    pcm_rom_id: *const c_char,
    pcm_rom_description: *const c_char,
    pcm_rom_sha1_digest: *const c_char,
}

impl RomInfo {
    fn empty() -> Self {
        RomInfo {
            control_rom_id: ptr::null(),
            control_rom_description: ptr::null(),
            control_rom_sha1_digest: ptr::null(),
            pcm_rom_id: ptr::null(),
            pcm_rom_description: ptr::null(),
            pcm_rom_sha1_digest: ptr::null(),
        }
    }
}

// mt32emu_report_handler_i_v0. Every callback the library finds NULL gets a
// default that prints to stdout, so the noisy ones are given quiet stubs.
// The va_list of printDebug is never touched, so it is declared as a pointer.
#[repr(C)]
struct ReportHandlerV0 {
    get_version_id: extern "C" fn(*const ReportHandlerV0) -> c_int,
    print_debug: extern "C" fn(*mut c_void, *const c_char, *mut c_void),
    on_error_control_rom: extern "C" fn(*mut c_void),
    on_error_pcm_rom: extern "C" fn(*mut c_void),
    show_lcd_message: extern "C" fn(*mut c_void, *const c_char),
    on_midi_message_played: Option<extern "C" fn(*mut c_void)>,
    on_midi_queue_overflow: Option<extern "C" fn(*mut c_void) -> c_int>,
    on_midi_system_realtime: Option<extern "C" fn(*mut c_void, u8)>,
    on_device_reset: Option<extern "C" fn(*mut c_void)>,
    on_device_reconfig: Option<extern "C" fn(*mut c_void)>,
    on_new_reverb_mode: Option<extern "C" fn(*mut c_void, u8)>,
    on_new_reverb_time: Option<extern "C" fn(*mut c_void, u8)>,
    on_new_reverb_level: Option<extern "C" fn(*mut c_void, u8)>,
    on_poly_state_changed: Option<extern "C" fn(*mut c_void, u8)>,
    on_program_changed: Option<extern "C" fn(*mut c_void, u8, *const c_char, *const c_char)>,
}

const RC_OK: c_int = 0;
const SRC_QUALITY_GOOD: c_int = 2;

unsafe extern "C" {
    fn mt32emu_get_library_version_string() -> *const c_char;
    fn mt32emu_get_machine_ids(ids: *mut *const c_char, size: usize) -> usize;
    fn mt32emu_get_rom_ids(ids: *mut *const c_char, size: usize, machine_id: *const c_char) -> usize;
    fn mt32emu_identify_rom_data(info: *mut RomInfo, data: *const u8, size: usize, machine_id: *const c_char) -> c_int;
    // The union mt32emu_report_handler_i holds one pointer, passed by value.
    fn mt32emu_create_context(handler: *const ReportHandlerV0, instance: *mut c_void) -> Context;
    fn mt32emu_free_context(context: Context);
    fn mt32emu_add_rom_data(context: Context, data: *const u8, size: usize, sha1: *const c_void) -> c_int;
    fn mt32emu_merge_and_add_rom_data(
        context: Context,
        data1: *const u8,
        size1: usize,
        sha1_1: *const c_void,
        data2: *const u8,
        size2: usize,
        sha1_2: *const c_void,
    ) -> c_int;
    fn mt32emu_get_rom_info(context: Context, info: *mut RomInfo);
    fn mt32emu_set_stereo_output_samplerate(context: Context, rate: f64);
    fn mt32emu_set_samplerate_conversion_quality(context: Context, quality: c_int);
    fn mt32emu_open_synth(context: Context) -> c_int;
    fn mt32emu_close_synth(context: Context);
    fn mt32emu_get_actual_stereo_output_samplerate(context: Context) -> u32;
    fn mt32emu_parse_stream(context: Context, stream: *const u8, length: u32);
    fn mt32emu_render_float(context: Context, stream: *mut f32, frames: u32);
    fn mt32emu_get_display_state(context: Context, target: *mut c_char, narrow_lcd: c_int) -> c_int;
}

extern "C" fn version_id(_: *const ReportHandlerV0) -> c_int {
    0
}
extern "C" fn quiet_debug(_: *mut c_void, _: *const c_char, _: *mut c_void) {}
extern "C" fn quiet(_: *mut c_void) {}
extern "C" fn lcd_message(instance: *mut c_void, message: *const c_char) {
    if instance.is_null() || message.is_null() {
        return;
    }
    // SAFETY: instance is the Sender boxed in Mt32 (or null), message a C string.
    let tx = unsafe { &*(instance as *const Sender<String>) };
    let text = unsafe { CStr::from_ptr(message) }.to_string_lossy().trim_end().to_string();
    let _ = tx.send(text);
}

static REPORT_HANDLER: ReportHandlerV0 = ReportHandlerV0 {
    get_version_id: version_id,
    print_debug: quiet_debug,
    on_error_control_rom: quiet,
    on_error_pcm_rom: quiet,
    show_lcd_message: lcd_message,
    on_midi_message_played: None,
    on_midi_queue_overflow: None,
    on_midi_system_realtime: None,
    on_device_reset: None,
    on_device_reconfig: None,
    on_new_reverb_mode: None,
    on_new_reverb_time: None,
    on_new_reverb_level: None,
    on_poly_state_changed: None,
    on_program_changed: None,
};

fn c_str(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        // SAFETY: the library returns static, NUL-terminated strings.
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}

pub fn library_version() -> String {
    c_str(unsafe { mt32emu_get_library_version_string() }).unwrap_or_default()
}

// ---- finding the ROMs --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Cm32l,
    Mt32,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::Cm32l => "CM-32L",
            Family::Mt32 => "MT-32",
        }
    }
}

#[derive(Clone)]
enum Image {
    Full(Vec<u8>),
    Pair(Vec<u8>, Vec<u8>),
}

/// A complete, working ROM set: a control ROM and a PCM ROM for one machine.
#[derive(Clone)]
pub struct RomSet {
    pub family: Family,
    pub machine: String,
    pub folder: PathBuf,
    pub control: String,
    images: Vec<Image>,
}

// LucasArts used the CM-32L's extra sounds, so a CM-32L comes first. Within
// a family, the versions games were written for come first. Machines that a
// later libmt32emu adds go last, in its order.
const PREFERRED: &[&str] = &[
    "cm32l_1_02",
    "cm32l_1_00",
    "cm32ln_1_00",
    "mt32_1_07",
    "mt32_1_06",
    "mt32_1_05",
    "mt32_1_04",
    "mt32_bluer",
    "mt32_2_07",
    "mt32_2_06",
    "mt32_2_04",
    "mt32_2_03",
];

fn family_of(machine: &str) -> Family {
    if machine.starts_with("cm32l") { Family::Cm32l } else { Family::Mt32 }
}

fn machines(mt32_only: bool) -> Vec<String> {
    let n = unsafe { mt32emu_get_machine_ids(ptr::null_mut(), 0) };
    let mut ids = vec![ptr::null::<c_char>(); n];
    let n = unsafe { mt32emu_get_machine_ids(ids.as_mut_ptr(), n) }.min(ids.len());
    let known: Vec<String> = ids[..n].iter().filter_map(|&p| c_str(p)).collect();
    let mut order: Vec<String> =
        PREFERRED.iter().filter(|m| known.iter().any(|k| k == *m)).map(|m| m.to_string()).collect();
    for k in &known {
        if !order.contains(k) {
            order.push(k.clone());
        }
    }
    order.sort_by_key(|m| family_of(m) == Family::Mt32); // stable: CM-32L first
    if mt32_only {
        order.retain(|m| family_of(m) == Family::Mt32);
    }
    order
}

fn rom_ids(machine: &str) -> Vec<String> {
    let m = CString::new(machine).unwrap();
    let n = unsafe { mt32emu_get_rom_ids(ptr::null_mut(), 0, m.as_ptr()) };
    let mut ids = vec![ptr::null::<c_char>(); n];
    let n = unsafe { mt32emu_get_rom_ids(ids.as_mut_ptr(), n, m.as_ptr()) }.min(ids.len());
    ids[..n].iter().filter_map(|&p| c_str(p)).collect()
}

// ROM images are 32 KB to 1 MB; anything far outside that is skipped
// without reading it.
const MIN_ROM: u64 = 16 * 1024;
const MAX_ROM: u64 = 2 * 1024 * 1024;

// The files in a folder and in the folders inside it (one level down).
fn candidate_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let Ok(meta) = fs::metadata(e.path()) else { continue }; // follows links
            if meta.is_dir() && depth == 0 {
                dirs.push((e.path(), 1));
            } else if meta.is_file() && (MIN_ROM..=MAX_ROM).contains(&meta.len()) {
                files.push(e.path());
            }
        }
    }
    files
}

/// Looks for a complete ROM set in one folder (and the folders inside it),
/// by the files' contents, not their names. A CM-32L set wins over an MT-32
/// set unless `mt32_only`.
pub fn find_in_folder(folder: &Path, mt32_only: bool) -> Option<RomSet> {
    // Only files the library knows as some ROM, or part of one.
    let files: Vec<Vec<u8>> = candidate_files(folder)
        .into_iter()
        .filter_map(|p| fs::read(p).ok())
        .filter(|d| identify(d, None).is_some())
        .collect();
    if files.is_empty() {
        return None;
    }
    for machine in machines(mt32_only) {
        let ids = rom_ids(&machine);
        // What each file is for this machine. The same data can be a whole
        // ROM for one machine and half of one for another: the MT-32's PCM
        // ROM is the first half of the CM-32L's.
        let parts: Vec<(&[u8], Rom)> =
            files.iter().filter_map(|d| identify(d, Some(&machine)).map(|r| (d.as_slice(), r))).collect();
        let (Some(control), Some(pcm)) = (pick(&parts, true, &ids), pick(&parts, false, &ids)) else { continue };
        let ctx = new_context();
        add(ctx, &control);
        add(ctx, &pcm);
        let mut info = RomInfo::empty();
        unsafe { mt32emu_get_rom_info(ctx, &mut info) };
        let description = c_str(info.control_rom_description);
        let complete = !info.pcm_rom_id.is_null();
        unsafe { mt32emu_free_context(ctx) };
        if let (Some(description), true) = (description, complete) {
            return Some(RomSet {
                family: family_of(&machine),
                machine,
                folder: folder.to_path_buf(),
                control: description,
                images: vec![control, pcm],
            });
        }
    }
    None
}

/// The first folder, in order, that holds a complete set.
pub fn find_roms(folders: &[PathBuf], mt32_only: bool) -> Option<RomSet> {
    folders.iter().find_map(|f| find_in_folder(f, mt32_only))
}

struct Rom {
    id: String,
    control: bool,
}

fn identify(data: &[u8], machine: Option<&str>) -> Option<Rom> {
    let m = machine.map(|m| CString::new(m).unwrap());
    let mut info = RomInfo::empty();
    unsafe {
        mt32emu_identify_rom_data(&mut info, data.as_ptr(), data.len(), m.as_ref().map_or(ptr::null(), |m| m.as_ptr()))
    };
    match (c_str(info.control_rom_id), c_str(info.pcm_rom_id)) {
        (Some(id), _) => Some(Rom { id, control: true }),
        (None, Some(id)) => Some(Rom { id, control: false }),
        _ => None,
    }
}

fn new_context() -> Context {
    unsafe { mt32emu_create_context(&REPORT_HANDLER, ptr::null_mut()) }
}

// The context keeps pointers into a whole image's data, which the caller
// keeps alive (Mt32 owns its copy).
fn add(ctx: Context, image: &Image) -> c_int {
    unsafe {
        match image {
            Image::Full(d) => mt32emu_add_rom_data(ctx, d.as_ptr(), d.len(), ptr::null()),
            Image::Pair(a, b) => {
                mt32emu_merge_and_add_rom_data(ctx, a.as_ptr(), a.len(), ptr::null(), b.as_ptr(), b.len(), ptr::null())
            }
        }
    }
}

// The ROM id a context ends up with after adding `image`, tried on its own.
fn added_id(image: &Image, control: bool) -> Option<String> {
    let ctx = new_context();
    let rc = add(ctx, image);
    let mut info = RomInfo::empty();
    unsafe { mt32emu_get_rom_info(ctx, &mut info) };
    let id = c_str(if control { info.control_rom_id } else { info.pcm_rom_id });
    unsafe { mt32emu_free_context(ctx) };
    if rc > RC_OK { id } else { None }
}

// A control (or PCM) ROM for the machine: a whole image if there is one,
// else two halves that make one.
fn pick(parts: &[(&[u8], Rom)], control: bool, machine_ids: &[String]) -> Option<Image> {
    let mine: Vec<&(&[u8], Rom)> = parts.iter().filter(|(_, r)| r.control == control).collect();
    for (data, rom) in &mine {
        let image = Image::Full(data.to_vec());
        if added_id(&image, control).as_deref() == Some(rom.id.as_str()) {
            return Some(image);
        }
    }
    for i in 0..mine.len() {
        for j in i + 1..mine.len() {
            let image = Image::Pair(mine[i].0.to_vec(), mine[j].0.to_vec());
            if added_id(&image, control).is_some_and(|id| machine_ids.contains(&id)) {
                return Some(image);
            }
        }
    }
    None
}

// ---- the synth -----------------------------------------------------------------

pub struct Mt32 {
    ctx: Context,
    rate: u32,
    _messages: Box<Sender<String>>,
    _images: Vec<Image>,
}

// The context is only used through &mut self, from one thread at a time.
unsafe impl Send for Mt32 {}

impl Mt32 {
    /// Opens an emulated MT-32 or CM-32L that renders at `rate` frames per
    /// second (libmt32emu converts from its own 32 kHz). Text the music
    /// puts on the MT-32's display goes to `messages`.
    pub fn open(roms: &RomSet, rate: u32, messages: Sender<String>) -> Result<Mt32, String> {
        let tx = Box::new(messages);
        let ctx = unsafe { mt32emu_create_context(&REPORT_HANDLER, &*tx as *const Sender<String> as *mut c_void) };
        if ctx.is_null() {
            return Err("libmt32emu could not start".into());
        }
        // Full images are used in place, so keep our own copy alive.
        let images = roms.images.clone();
        for image in &images {
            add(ctx, image);
        }
        unsafe {
            mt32emu_set_stereo_output_samplerate(ctx, rate as f64);
            mt32emu_set_samplerate_conversion_quality(ctx, SRC_QUALITY_GOOD);
        }
        if unsafe { mt32emu_open_synth(ctx) } != RC_OK {
            unsafe { mt32emu_free_context(ctx) };
            return Err(format!(
                "the {} ROMs in {} did not start the synth",
                roms.family.name(),
                roms.folder.display()
            ));
        }
        let rate = unsafe { mt32emu_get_actual_stereo_output_samplerate(ctx) };
        Ok(Mt32 { ctx, rate, _messages: tx, _images: images })
    }
}

impl Drop for Mt32 {
    fn drop(&mut self) {
        unsafe {
            mt32emu_close_synth(self.ctx);
            mt32emu_free_context(self.ctx);
        }
    }
}

impl Synth for Mt32 {
    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn feed(&mut self, midi: &[u8]) {
        for chunk in midi.chunks(u32::MAX as usize) {
            unsafe { mt32emu_parse_stream(self.ctx, chunk.as_ptr(), chunk.len() as u32) };
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        unsafe { mt32emu_render_float(self.ctx, out.as_mut_ptr(), (out.len() / 2) as u32) };
    }

    fn display(&mut self) -> Option<(String, bool)> {
        let mut buf = [0 as c_char; 21];
        let led = unsafe { mt32emu_get_display_state(self.ctx, buf.as_mut_ptr(), 0) } != 0;
        let text = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned();
        Some((text, led))
    }
}
