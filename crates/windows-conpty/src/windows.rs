use std::collections::{BTreeMap, VecDeque};
use std::ffi::{OsStr, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::process::Command;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use win32job::{ExtendedLimitInfo, Job};
use windows_sys::Win32::Foundation::{
    HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, PSEUDOCONSOLE_INHERIT_CURSOR,
};
use windows_sys::Win32::System::IO::CancelSynchronousIo;
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
};

const PSEUDOCONSOLE_RESIZE_QUIRK: u32 = 0x2;
const PSEUDOCONSOLE_WIN32_INPUT_MODE: u32 = 0x4;
const CANCEL_JOIN_RESERVE: Duration = Duration::from_millis(50);
const OUTPUT_READ_CHUNK_BYTES: usize = 16 * 1_024;
pub(crate) const OUTPUT_QUEUE_CAPACITY_BYTES: usize = 64 * 1_024;

// The ConPTY and process-attribute ownership pattern is derived from WezTerm
// and the OpenAI Codex PTY utility (MIT license).
// Copyright (c) 2018-Present Wez Furlong
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

pub struct SpawnedConPty {
    pub child: ConPtyChild,
    pub reader: ConPtyReader,
    pub writer: ConPtyWriter,
    pub control: ConPtyControl,
}

pub fn spawn(command: &Command, cols: u16, rows: u16) -> io::Result<SpawnedConPty> {
    let (input_server, input_client) = create_pipe()?;
    let (output_client, output_server) = create_pipe()?;
    let pseudo = PseudoConsole::new(cols, rows, &input_server, &output_server)?;
    let (reader, drain) = start_output_drain(output_client)?;
    let job = create_job()?;
    let child = spawn_process(command, pseudo.handle())?;
    Ok(SpawnedConPty {
        child,
        reader,
        writer: ConPtyWriter(File::from(input_client)),
        control: ConPtyControl {
            pseudo: Some(pseudo),
            input_server: Some(input_server),
            output_server: Some(output_server),
            drain: Some(drain),
            job: Some(job),
            assigned: false,
        },
    })
}

pub struct ConPtyChild {
    process: OwnedHandle,
    process_id: u32,
}

impl ConPtyChild {
    #[must_use]
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn try_wait(&mut self) -> io::Result<Option<i32>> {
        // SAFETY: the process handle is owned for this zero-time status probe.
        match unsafe { WaitForSingleObject(raw_handle(&self.process), 0) } {
            WAIT_TIMEOUT => return Ok(None),
            WAIT_OBJECT_0 => {}
            WAIT_FAILED => return Err(io::Error::last_os_error()),
            result => {
                return Err(io::Error::other(format!(
                    "unexpected Windows process wait result: {result}"
                )));
            }
        }
        let mut code = 0_u32;
        // SAFETY: the zero-time wait proved the owned process is signaled, and
        // code is writable. Exit code 259 is therefore a literal terminal code.
        if unsafe { GetExitCodeProcess(raw_handle(&self.process), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(code as i32))
    }

    pub fn kill(&mut self) -> io::Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        // SAFETY: the process handle remains owned for this call.
        if unsafe { TerminateProcess(raw_handle(&self.process), 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn reap(&mut self) -> io::Result<()> {
        // SAFETY: the process handle remains owned. The caller observes exit first.
        let result = unsafe { WaitForSingleObject(raw_handle(&self.process), INFINITE) };
        if result == u32::MAX {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

pub struct ConPtyWriter(File);

impl Write for ConPtyWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

struct OutputQueueState {
    chunks: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    reserved_bytes: usize,
    error: Option<io::Error>,
    closed: bool,
    receiver_alive: bool,
}

struct OutputQueue {
    state: Mutex<OutputQueueState>,
    readable: Condvar,
    writable: Condvar,
}

#[derive(Clone, Copy)]
enum QueueReservation {
    Reserved,
    Discard,
    Cancelled,
}

impl OutputQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(OutputQueueState {
                chunks: VecDeque::new(),
                queued_bytes: 0,
                reserved_bytes: 0,
                error: None,
                closed: false,
                receiver_alive: true,
            }),
            readable: Condvar::new(),
            writable: Condvar::new(),
        }
    }

    fn reserve_read(&self, cancelling: &AtomicBool) -> QueueReservation {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        while state.receiver_alive
            && !cancelling.load(Ordering::Acquire)
            && state.queued_bytes + state.reserved_bytes + OUTPUT_READ_CHUNK_BYTES
                > OUTPUT_QUEUE_CAPACITY_BYTES
        {
            state = self
                .writable
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        if !state.receiver_alive {
            QueueReservation::Discard
        } else if state.queued_bytes + state.reserved_bytes + OUTPUT_READ_CHUNK_BYTES
            > OUTPUT_QUEUE_CAPACITY_BYTES
            && cancelling.load(Ordering::Acquire)
        {
            QueueReservation::Cancelled
        } else {
            state.reserved_bytes += OUTPUT_READ_CHUNK_BYTES;
            self.readable.notify_all();
            QueueReservation::Reserved
        }
    }

    fn commit_read(&self, bytes: Vec<u8>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.reserved_bytes = state.reserved_bytes.saturating_sub(OUTPUT_READ_CHUNK_BYTES);
        if state.receiver_alive {
            state.queued_bytes += bytes.len();
            state.chunks.push_back(bytes);
            self.readable.notify_one();
        }
        self.writable.notify_all();
    }

    fn release_reservation(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.reserved_bytes = state.reserved_bytes.saturating_sub(OUTPUT_READ_CHUNK_BYTES);
        self.writable.notify_all();
    }

    fn finish(&self, error: Option<io::Error>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if error.is_some() {
            state.error = error;
        }
        state.closed = true;
        self.readable.notify_all();
        self.writable.notify_all();
    }

    fn wake_producer(&self) {
        self.writable.notify_all();
    }

    #[cfg(test)]
    fn wait_for_full(&self, deadline: Instant) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        while state.queued_bytes + state.reserved_bytes < OUTPUT_QUEUE_CAPACITY_BYTES {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, timeout) = self
                .readable
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if timeout.timed_out() {
                break;
            }
        }
        state.queued_bytes + state.reserved_bytes == OUTPUT_QUEUE_CAPACITY_BYTES
    }

    #[cfg(test)]
    fn queued_bytes(&self) -> usize {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.queued_bytes + state.reserved_bytes
    }
}

pub struct ConPtyReader {
    queue: Arc<OutputQueue>,
    pending: Vec<u8>,
    offset: usize,
}

impl Read for ConPtyReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            if self.offset < self.pending.len() {
                let count = buffer.len().min(self.pending.len() - self.offset);
                buffer[..count].copy_from_slice(&self.pending[self.offset..self.offset + count]);
                self.offset += count;
                if self.offset == self.pending.len() {
                    self.pending.clear();
                    self.offset = 0;
                }
                return Ok(count);
            }
            let mut state = self
                .queue
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            while state.chunks.is_empty() && !state.closed {
                state = self
                    .queue
                    .readable
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
            if let Some(bytes) = state.chunks.pop_front() {
                state.queued_bytes = state.queued_bytes.saturating_sub(bytes.len());
                self.queue.writable.notify_all();
                drop(state);
                self.pending = bytes;
            } else if let Some(error) = state.error.take() {
                return Err(error);
            } else {
                return Ok(0);
            }
        }
    }
}

impl Drop for ConPtyReader {
    fn drop(&mut self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.receiver_alive = false;
        state.chunks.clear();
        state.queued_bytes = 0;
        self.queue.writable.notify_all();
        self.queue.readable.notify_all();
    }
}

pub struct ConPtyControl {
    pseudo: Option<PseudoConsole>,
    input_server: Option<OwnedHandle>,
    output_server: Option<OwnedHandle>,
    drain: Option<OutputDrain>,
    job: Option<Job>,
    assigned: bool,
}

impl ConPtyControl {
    pub fn assign_job(&mut self, child: &ConPtyChild) -> io::Result<()> {
        if self.assigned {
            return Err(io::Error::other(
                "ConPTY process was already assigned to its Job",
            ));
        }
        self.job
            .as_ref()
            .ok_or_else(|| io::Error::other("ConPTY Job unavailable before assignment"))?
            .assign_process(raw_handle(&child.process) as isize)
            .map_err(|error| io::Error::other(format!("assign ConPTY process Job: {error}")))?;
        self.assigned = true;
        Ok(())
    }

    pub fn terminate_tree(&mut self) {
        drop(self.job.take());
    }

    #[must_use]
    pub fn tree_terminated(&self) -> bool {
        self.job.is_none()
    }

    pub fn close_io_before(&mut self, deadline: Instant) -> io::Result<()> {
        drop(self.pseudo.take());
        drop(self.input_server.take());
        drop(self.output_server.take());
        if let Some(mut drain) = self.drain.take()
            && let Err(error) = drain.close_before(deadline)
        {
            self.drain = Some(drain);
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for ConPtyControl {
    fn drop(&mut self) {
        drop(self.job.take());
        let _ = self.close_io_before(Instant::now() + Duration::from_millis(100));
    }
}

struct PseudoConsole(HPCON);

impl PseudoConsole {
    fn new(cols: u16, rows: u16, input: &OwnedHandle, output: &OwnedHandle) -> io::Result<Self> {
        let cols = i16::try_from(cols)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ConPTY cols exceed i16"))?;
        let rows = i16::try_from(rows)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ConPTY rows exceed i16"))?;
        let mut handle = 0;
        // SAFETY: the owned pipe handles are retained until ClosePseudoConsole.
        let result = unsafe {
            CreatePseudoConsole(
                COORD { X: cols, Y: rows },
                raw_handle(input),
                raw_handle(output),
                PSEUDOCONSOLE_INHERIT_CURSOR
                    | PSEUDOCONSOLE_RESIZE_QUIRK
                    | PSEUDOCONSOLE_WIN32_INPUT_MODE,
                &mut handle,
            )
        };
        if result < 0 {
            Err(io::Error::from_raw_os_error(result))
        } else {
            Ok(Self(handle))
        }
    }

    fn handle(&self) -> HPCON {
        self.0
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns and closes HPCON exactly once.
        unsafe { ClosePseudoConsole(self.0) };
    }
}

struct AttributeList {
    storage: Vec<usize>,
    initialized: bool,
}

impl AttributeList {
    fn for_conpty(conpty: HPCON) -> io::Result<Self> {
        let mut bytes = 0_usize;
        // SAFETY: documented sizing call with a null list.
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut result = Self {
            storage: vec![0; bytes.div_ceil(mem::size_of::<usize>())],
            initialized: false,
        };
        // SAFETY: the allocation is aligned and at least bytes long.
        if unsafe { InitializeProcThreadAttributeList(result.pointer(), 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        result.initialized = true;
        // SAFETY: the initialized list stays live through CreateProcessW.
        if unsafe {
            UpdateProcThreadAttribute(
                result.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                conpty as *const c_void,
                mem::size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(result)
    }

    fn pointer(&mut self) -> *mut c_void {
        self.storage.as_mut_ptr().cast()
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: the list was initialized once and remains allocated.
            unsafe { DeleteProcThreadAttributeList(self.pointer()) };
        }
    }
}

fn spawn_process(command: &Command, conpty: HPCON) -> io::Result<ConPtyChild> {
    let mut attributes = AttributeList::for_conpty(conpty)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    startup.lpAttributeList = attributes.pointer();
    let program = wide_null(command.get_program())?;
    let mut command_line = build_command_line(command)?;
    let cwd = command
        .get_current_dir()
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_dir()?);
    let cwd = wide_null(cwd.as_os_str())?;
    let environment = build_environment(command)?;
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: all buffers and the initialized attribute list live through this call.
    let created = unsafe {
        CreateProcessW(
            program.as_ptr(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            ptr::from_ref(&startup.StartupInfo),
            &mut process,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateProcessW returned two new uniquely owned handles.
    let thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
    // SAFETY: the process handle ownership transfers to ConPtyChild.
    let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    drop(thread);
    Ok(ConPtyChild {
        process: process_handle,
        process_id: process.dwProcessId,
    })
}

fn build_command_line(command: &Command) -> io::Result<Vec<u16>> {
    let mut result = Vec::new();
    append_quoted(command.get_program(), &mut result)?;
    for argument in command.get_args() {
        result.push(b' ' as u16);
        append_quoted(argument, &mut result)?;
    }
    result.push(0);
    Ok(result)
}

fn append_quoted(argument: &OsStr, output: &mut Vec<u16>) -> io::Result<()> {
    let argument = argument.encode_wide().collect::<Vec<_>>();
    if argument.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command contains NUL",
        ));
    }
    if !argument.is_empty()
        && !argument
            .iter()
            .any(|unit| matches!(*unit, 0x20 | 0x09 | 0x0a | 0x0b | 0x22))
    {
        output.extend(argument);
        return Ok(());
    }
    output.push(b'"' as u16);
    let mut index = 0;
    while index < argument.len() {
        let start = index;
        while index < argument.len() && argument[index] == b'\\' as u16 {
            index += 1;
        }
        let slashes = index - start;
        if index == argument.len() {
            output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
            break;
        }
        let escaped = argument[index] == b'"' as u16;
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if escaped { slashes * 2 + 1 } else { slashes },
        ));
        output.push(argument[index]);
        index += 1;
    }
    output.push(b'"' as u16);
    Ok(())
}

fn build_environment(command: &Command) -> io::Result<Vec<u16>> {
    let mut environment = std::env::vars_os().collect::<BTreeMap<_, _>>();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            environment.insert(key.to_os_string(), value.to_os_string());
        } else {
            environment.remove(key);
        }
    }
    let mut entries = environment.into_iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| key.to_string_lossy().to_lowercase());
    let mut block = Vec::new();
    for (key, value) in entries {
        let mut entry = key;
        entry.push("=");
        entry.push(value);
        let encoded = entry.encode_wide().collect::<Vec<_>>();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "environment contains NUL",
            ));
        }
        block.extend(encoded);
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn wide_null(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut encoded = value.encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

fn create_job() -> io::Result<Job> {
    let mut limits = ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    Job::create_with_limit_info(&limits)
        .map_err(|error| io::Error::other(format!("create ConPTY Job: {error}")))
}

fn raw_handle(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle()
}

fn create_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    // SAFETY: both out-pointers are writable and receive unique handles.
    if unsafe { CreatePipe(&mut read, &mut write, ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful CreatePipe transfers both handle ownerships.
    let read = unsafe { OwnedHandle::from_raw_handle(read) };
    // SAFETY: this is the distinct write handle returned by CreatePipe.
    let write = unsafe { OwnedHandle::from_raw_handle(write) };
    Ok((read, write))
}

struct DrainCompletion {
    complete: Mutex<bool>,
    changed: Condvar,
}

impl DrainCompletion {
    fn wait_until(&self, deadline: Instant) -> bool {
        let mut complete = self
            .complete
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while !*complete {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, timeout) = self
                .changed
                .wait_timeout(complete, remaining)
                .unwrap_or_else(|error| error.into_inner());
            complete = next;
            if timeout.timed_out() {
                break;
            }
        }
        *complete
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReaderPhase {
    BetweenReads,
    Reading,
    Complete,
}

struct ReaderProtocol {
    phase: Mutex<ReaderPhase>,
    changed: Condvar,
    cancellation_requested: AtomicBool,
}

impl ReaderProtocol {
    fn new() -> Self {
        Self {
            phase: Mutex::new(ReaderPhase::BetweenReads),
            changed: Condvar::new(),
            cancellation_requested: AtomicBool::new(false),
        }
    }

    fn set_phase(&self, phase: ReaderPhase) {
        *self.phase.lock().unwrap_or_else(|error| error.into_inner()) = phase;
        self.changed.notify_all();
    }

    fn phase(&self) -> ReaderPhase {
        *self.phase.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn request_cancellation(&self) {
        self.cancellation_requested.store(true, Ordering::Release);
        self.changed.notify_all();
    }

    fn wait_for_change(&self, duration: Duration) {
        let phase = self.phase.lock().unwrap_or_else(|error| error.into_inner());
        let _ = self
            .changed
            .wait_timeout(phase, duration)
            .unwrap_or_else(|error| error.into_inner());
    }

    #[cfg(test)]
    fn wait_for_cancellation(&self, deadline: Instant) -> bool {
        let mut phase = self.phase.lock().unwrap_or_else(|error| error.into_inner());
        while !self.cancellation_requested.load(Ordering::Acquire) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, timeout) = self
                .changed
                .wait_timeout(phase, remaining)
                .unwrap_or_else(|error| error.into_inner());
            phase = next;
            if timeout.timed_out() {
                break;
            }
        }
        self.cancellation_requested.load(Ordering::Acquire)
    }
}

struct BetweenReadsPauseState {
    paused: bool,
    released: bool,
}

struct BetweenReadsPause {
    state: Mutex<BetweenReadsPauseState>,
    changed: Condvar,
}

impl BetweenReadsPause {
    fn pause(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.paused = true;
        self.changed.notify_all();
        while !state.released {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

#[cfg(test)]
pub(crate) struct BetweenReadsControl {
    pause: Arc<BetweenReadsPause>,
    protocol: Arc<ReaderProtocol>,
}

#[cfg(test)]
impl BetweenReadsControl {
    pub(crate) fn wait_until_paused(&self, duration: Duration) {
        let deadline = Instant::now() + duration;
        let mut state = self
            .pause
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while !state.paused {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "reader did not pause between reads");
            let (next, _) = self
                .pause
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
        }
    }

    pub(crate) fn wait_until_cancellation_requested(&self, duration: Duration) {
        assert!(
            self.protocol
                .wait_for_cancellation(Instant::now() + duration),
            "closer did not request between-read cancellation"
        );
    }

    pub(crate) fn release(&self) {
        let mut state = self
            .pause
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.released = true;
        self.pause.changed.notify_all();
    }
}

pub(crate) struct OutputDrain {
    handle: Option<thread::JoinHandle<()>>,
    completion: Arc<DrainCompletion>,
    protocol: Arc<ReaderProtocol>,
    queue: Arc<OutputQueue>,
}

impl OutputDrain {
    #[cfg(test)]
    pub(crate) fn wait_for(&self, duration: Duration) -> bool {
        self.completion.wait_until(Instant::now() + duration)
    }

    #[cfg(test)]
    pub(crate) fn wait_for_queue_full(&self, duration: Duration) -> bool {
        self.queue.wait_for_full(Instant::now() + duration)
    }

    #[cfg(test)]
    pub(crate) fn queued_bytes(&self) -> usize {
        self.queue.queued_bytes()
    }

    pub(crate) fn close_before(&mut self, deadline: Instant) -> io::Result<()> {
        let cancel_at = deadline
            .checked_sub(CANCEL_JOIN_RESERVE)
            .unwrap_or_else(Instant::now);
        if !self.completion.wait_until(cancel_at) {
            self.protocol.request_cancellation();
            self.queue.wake_producer();
            while !self.completion.wait_until(Instant::now()) && Instant::now() < deadline {
                if self.protocol.phase() == ReaderPhase::Reading
                    && let Some(handle) = self.handle.as_ref()
                {
                    // SAFETY: JoinHandle owns the target thread through cancellation/join.
                    // Repeating this call closes the race where cancellation was first
                    // requested between two synchronous reads.
                    unsafe { CancelSynchronousIo(handle.as_raw_handle()) };
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                self.protocol
                    .wait_for_change(remaining.min(Duration::from_millis(2)));
            }
        }
        if !self.completion.wait_until(deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ConPTY output drain exceeded cleanup deadline",
            ));
        }
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| io::Error::other("ConPTY output drain panicked"))?;
        }
        Ok(())
    }
}

impl Drop for OutputDrain {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let _ = self.close_before(Instant::now() + Duration::from_millis(100));
        }
    }
}

fn start_output_drain(read_end: OwnedHandle) -> io::Result<(ConPtyReader, OutputDrain)> {
    start_output_drain_inner(read_end, None)
}

fn start_output_drain_inner(
    read_end: OwnedHandle,
    pause_after_first_read: Option<Arc<BetweenReadsPause>>,
) -> io::Result<(ConPtyReader, OutputDrain)> {
    let queue = Arc::new(OutputQueue::new());
    let completion = Arc::new(DrainCompletion {
        complete: Mutex::new(false),
        changed: Condvar::new(),
    });
    let protocol = Arc::new(ReaderProtocol::new());
    let thread_completion = Arc::clone(&completion);
    let thread_protocol = Arc::clone(&protocol);
    let thread_queue = Arc::clone(&queue);
    let handle = thread::Builder::new()
        .name("minimax-conpty-output".to_owned())
        .spawn(move || {
            let mut read = File::from(read_end);
            let mut buffer = vec![0_u8; OUTPUT_READ_CHUNK_BYTES];
            let mut discard = false;
            let mut first_read = true;
            loop {
                let reserved = if discard {
                    QueueReservation::Discard
                } else {
                    thread_queue.reserve_read(&thread_protocol.cancellation_requested)
                };
                match reserved {
                    QueueReservation::Cancelled => break,
                    QueueReservation::Discard => discard = true,
                    QueueReservation::Reserved => {}
                }
                thread_protocol.set_phase(ReaderPhase::Reading);
                let result = read.read(&mut buffer);
                thread_protocol.set_phase(ReaderPhase::BetweenReads);
                match result {
                    Ok(0) => {
                        if matches!(reserved, QueueReservation::Reserved) {
                            thread_queue.release_reservation();
                        }
                        break;
                    }
                    Ok(count) => {
                        if matches!(reserved, QueueReservation::Reserved) {
                            thread_queue.commit_read(buffer[..count].to_vec());
                        }
                        if first_read {
                            first_read = false;
                            if let Some(pause) = pause_after_first_read.as_ref() {
                                pause.pause();
                            }
                        }
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::BrokenPipe
                            || thread_protocol
                                .cancellation_requested
                                .load(Ordering::Acquire) =>
                    {
                        if matches!(reserved, QueueReservation::Reserved) {
                            thread_queue.release_reservation();
                        }
                        break;
                    }
                    Err(error) => {
                        if matches!(reserved, QueueReservation::Reserved) {
                            thread_queue.release_reservation();
                        }
                        thread_queue.finish(Some(error));
                        break;
                    }
                }
            }
            thread_queue.finish(None);
            thread_protocol.set_phase(ReaderPhase::Complete);
            let mut complete = thread_completion
                .complete
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            *complete = true;
            thread_completion.changed.notify_all();
        })?;
    Ok((
        ConPtyReader {
            queue: Arc::clone(&queue),
            pending: Vec::new(),
            offset: 0,
        },
        OutputDrain {
            handle: Some(handle),
            completion,
            protocol,
            queue,
        },
    ))
}

#[cfg(test)]
pub(crate) fn create_test_pipe() -> io::Result<(OwnedHandle, File)> {
    let (read, write) = create_pipe()?;
    Ok((read, File::from(write)))
}

#[cfg(test)]
pub(crate) fn test_output_drain(read: OwnedHandle) -> io::Result<(ConPtyReader, OutputDrain)> {
    start_output_drain(read)
}

#[cfg(test)]
pub(crate) fn test_output_drain_paused_between_reads(
    read: OwnedHandle,
) -> io::Result<(ConPtyReader, OutputDrain, BetweenReadsControl)> {
    let pause = Arc::new(BetweenReadsPause {
        state: Mutex::new(BetweenReadsPauseState {
            paused: false,
            released: false,
        }),
        changed: Condvar::new(),
    });
    let (reader, drain) = start_output_drain_inner(read, Some(Arc::clone(&pause)))?;
    let control = BetweenReadsControl {
        pause,
        protocol: Arc::clone(&drain.protocol),
    };
    Ok((reader, drain, control))
}

#[cfg(test)]
pub(crate) fn spawn_test_child(command: &mut Command) -> io::Result<ConPtyChild> {
    use std::os::windows::io::AsHandle;

    let child = command.spawn()?;
    let process_id = child.id();
    let process = child.as_handle().try_clone_to_owned()?;
    drop(child);
    Ok(ConPtyChild {
        process,
        process_id,
    })
}
