use alloc::{boxed::Box, string::String, sync::Arc};
use core::ops::Deref;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicU8, Ordering};
use core::{alloc::Layout, cell::UnsafeCell, fmt, ptr::NonNull};

#[cfg(feature = "preempt")]
use core::sync::atomic::AtomicUsize;

use axhal::arch::TaskContext;
use memory_addr::{align_up_4k, VirtAddr};

use crate::{AxRunQueue, AxTask, AxTaskRef, WaitQueue};

use axhal::arch::TrapFrame;

/// A unique identifier for a thread.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TaskId(u64);

/// The possible states of a task.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TaskState {
    Running = 1,
    Ready = 2,
    Blocked = 3,
    Exited = 4,
}

pub struct ProcessInner {
    process_id: AtomicU64,
    /// 是否是所属进程下的主线程
    is_leader: AtomicBool,

    /// 初始化的trap上下文
    pub trap_frame: UnsafeCell<TrapFrame>,

    pub page_table_token: usize,

    set_child_tid: AtomicU64,

    clear_child_tid: AtomicU64,
}

pub struct VcpuInner {
    // Refer to `Vcpu` struct in crates/hypercraft/arch/${ARCH}/vcpu.rs
}

#[derive(Debug)]
pub enum TaskType {
    /// ArceOS task.
    Task,
    /// User process.
    Process,
    /// Virtual CPU.
    Vcpu,
}

/// The inner task structure.
pub struct TaskInner {
    id: TaskId,
    name: String,
    is_idle: bool,
    is_init: bool,

    entry: Option<*mut dyn FnOnce()>,
    state: AtomicU8,

    in_wait_queue: AtomicBool,
    #[cfg(feature = "irq")]
    in_timer_list: AtomicBool,

    #[cfg(feature = "preempt")]
    need_resched: AtomicBool,
    #[cfg(feature = "preempt")]
    preempt_disable_count: AtomicUsize,

    exit_code: AtomicI32,
    wait_for_exit: WaitQueue,

    kstack: Option<TaskStack>,
    /// On unikernel, this field stores the context of task.
    /// On Process, this field stores the kernel context of thread.
    /// On Vcpu, this field stores the kernel context of root-mode.
    ctx: UnsafeCell<TaskContext>,

    task_type: TaskType,
    process_inner: Option<ProcessInner>,
    vcpu_inner: Option<Box<VcpuInner>>,
}

impl TaskId {
    pub fn new() -> Self {
        static ID_COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Convert the task ID to a `u64`.
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u8> for TaskState {
    #[inline]
    fn from(state: u8) -> Self {
        match state {
            1 => Self::Running,
            2 => Self::Ready,
            3 => Self::Blocked,
            4 => Self::Exited,
            _ => unreachable!(),
        }
    }
}

unsafe impl Send for TaskInner {}
unsafe impl Sync for TaskInner {}

impl TaskInner {
    /// Gets the ID of the task.
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// Gets the name of the task.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Get a combined string of the task ID and name.
    pub fn id_name(&self) -> alloc::string::String {
        alloc::format!("Task({}, {:?})", self.id.as_u64(), self.name)
    }

    /// Wait for the task to exit, and return the exit code.
    ///
    /// It will return immediately if the task has already exited (but not dropped).
    pub fn join(&self) -> Option<i32> {
        self.wait_for_exit
            .wait_until(|| self.state() == TaskState::Exited);
        Some(self.exit_code.load(Ordering::Acquire))
    }

    /// 获取内核栈栈顶
    #[inline]
    pub fn get_kernel_stack_top(&self) -> Option<usize> {
        if let Some(kstack) = &self.kstack {
            return Some(kstack.top().as_usize());
        }
        None
    }
}

// private methods
impl TaskInner {
    const fn new_common(id: TaskId, name: String, task_type: TaskType) -> Self {
        Self {
            id,
            name,
            is_idle: false,
            is_init: false,
            entry: None,
            state: AtomicU8::new(TaskState::Ready as u8),
            in_wait_queue: AtomicBool::new(false),
            #[cfg(feature = "irq")]
            in_timer_list: AtomicBool::new(false),
            #[cfg(feature = "preempt")]
            need_resched: AtomicBool::new(false),
            #[cfg(feature = "preempt")]
            preempt_disable_count: AtomicUsize::new(0),
            exit_code: AtomicI32::new(0),
            wait_for_exit: WaitQueue::new(),
            kstack: None,
            ctx: UnsafeCell::new(TaskContext::new()),
            task_type,
            process_inner: None,
            vcpu_inner: None,
        }
    }

    pub(crate) fn new<F>(entry: F, name: String, stack_size: usize) -> AxTaskRef
    where
        F: FnOnce() + Send + 'static,
    {
        let mut t = Self::new_common(TaskId::new(), name, TaskType::Task);
        debug!("new task: {}", t.id_name());
        let kstack = TaskStack::alloc(align_up_4k(stack_size));
        t.entry = Some(Box::into_raw(Box::new(entry)));
        t.ctx.get_mut().init(task_entry as usize, kstack.top());
        t.kstack = Some(kstack);
        if t.name == "idle" {
            t.is_idle = true;
        }
        Arc::new(AxTask::new(t))
    }

    pub(crate) fn new_init(name: String) -> AxTaskRef {
        // init_task does not change PC and SP, so `entry` and `kstack` fields are not used.
        let mut t = Self::new_common(TaskId::new(), name, TaskType::Task);
        t.is_init = true;
        if t.name == "idle" {
            t.is_idle = true;
        }
        Arc::new(AxTask::new(t))
    }

    #[inline]
    pub(crate) fn state(&self) -> TaskState {
        self.state.load(Ordering::Acquire).into()
    }

    #[inline]
    pub(crate) fn set_state(&self, state: TaskState) {
        self.state.store(state as u8, Ordering::Release)
    }

    #[inline]
    pub(crate) fn is_running(&self) -> bool {
        matches!(self.state(), TaskState::Running)
    }

    #[inline]
    pub(crate) fn is_ready(&self) -> bool {
        matches!(self.state(), TaskState::Ready)
    }

    #[inline]
    pub(crate) fn is_blocked(&self) -> bool {
        matches!(self.state(), TaskState::Blocked)
    }

    #[inline]
    pub(crate) const fn is_init(&self) -> bool {
        self.is_init
    }

    #[inline]
    pub(crate) const fn is_idle(&self) -> bool {
        self.is_idle
    }

    #[inline]
    pub(crate) fn in_wait_queue(&self) -> bool {
        self.in_wait_queue.load(Ordering::Acquire)
    }

    #[inline]
    pub(crate) fn set_in_wait_queue(&self, in_wait_queue: bool) {
        self.in_wait_queue.store(in_wait_queue, Ordering::Release);
    }

    #[inline]
    #[cfg(feature = "irq")]
    pub(crate) fn in_timer_list(&self) -> bool {
        self.in_timer_list.load(Ordering::Acquire)
    }

    #[inline]
    #[cfg(feature = "irq")]
    pub(crate) fn set_in_timer_list(&self, in_timer_list: bool) {
        self.in_timer_list.store(in_timer_list, Ordering::Release);
    }

    #[inline]
    #[cfg(feature = "preempt")]
    pub(crate) fn set_preempt_pending(&self, pending: bool) {
        self.need_resched.store(pending, Ordering::Release)
    }

    #[inline]
    #[cfg(feature = "preempt")]
    pub(crate) fn can_preempt(&self, current_disable_count: usize) -> bool {
        self.preempt_disable_count.load(Ordering::Acquire) == current_disable_count
    }

    #[inline]
    #[cfg(feature = "preempt")]
    pub(crate) fn disable_preempt(&self) {
        self.preempt_disable_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    #[cfg(feature = "preempt")]
    pub(crate) fn enable_preempt(&self, resched: bool) {
        if self.preempt_disable_count.fetch_sub(1, Ordering::Relaxed) == 1 && resched {
            // If current task is pending to be preempted, do rescheduling.
            Self::current_check_preempt_pending();
        }
    }

    #[cfg(feature = "preempt")]
    fn current_check_preempt_pending() {
        let curr = crate::current();
        if curr.need_resched.load(Ordering::Acquire) && curr.can_preempt(0) {
            let mut rq = crate::RUN_QUEUE.lock();
            if curr.need_resched.load(Ordering::Acquire) {
                rq.preempt_resched();
            }
        }
    }

    pub(crate) fn notify_exit(&self, exit_code: i32, rq: &mut AxRunQueue) {
        self.exit_code.store(exit_code, Ordering::Release);
        self.wait_for_exit.notify_all_locked(false, rq);
    }

    #[inline]
    pub(crate) const unsafe fn ctx_mut_ptr(&self) -> *mut TaskContext {
        self.ctx.get()
    }
}

impl TaskInner {
    pub fn new_process<F>(
        entry: F,
        name: String,
        stack_size: usize,
        process_id: u64,
        page_table_token: usize,
    ) -> AxTaskRef
    where
        F: FnOnce() + Send + 'static,
    {
        let mut t = Self::new_common(TaskId::new(), name, TaskType::Process);
        debug!("new task: {}", t.id_name());
        let kstack = TaskStack::alloc(align_up_4k(stack_size));
        t.entry = Some(Box::into_raw(Box::new(entry)));
        t.ctx.get_mut().init(task_entry as usize, kstack.top());
        t.kstack = Some(kstack);
        if t.name == "idle" {
            t.is_idle = true;
        }

        let process_inner = ProcessInner {
            process_id: AtomicU64::new(process_id),
            is_leader: AtomicBool::new(true),
            trap_frame: UnsafeCell::new(TrapFrame::default()),
            page_table_token: 0,
            set_child_tid: AtomicU64::new(0),
            clear_child_tid: AtomicU64::new(0),
        };

        t.process_inner = Some(process_inner);

        Arc::new(AxTask::new(t))
    }

    pub fn set_child_tid(&self, tid: usize) {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .set_child_tid
            .store(tid as u64, Ordering::Release)
    }

    pub fn set_clear_child_tid(&self, tid: usize) {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .clear_child_tid
            .store(tid as u64, Ordering::Release)
    }

    pub fn get_clear_child_tid(&self) -> usize {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .clear_child_tid
            .load(Ordering::Acquire) as usize
    }

    #[inline]
    pub fn get_page_table_token(&self) -> usize {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .page_table_token
    }

    #[inline]
    pub fn get_process_id(&self) -> u64 {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .process_id
            .load(Ordering::Acquire)
    }

    #[inline]
    pub fn set_process_id(&self, process_id: u64) {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .process_id
            .store(process_id, Ordering::Release);
    }

    /// 获取内核栈的第一个trap上下文
    #[inline]
    pub fn get_first_trap_frame(&self) -> *mut TrapFrame {
        if let Some(kstack) = &self.kstack {
            return kstack.get_first_trap_frame();
        }
        unreachable!("get_first_trap_frame: kstack is None");
    }

    pub fn set_leader(&self, is_lead: bool) {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .is_leader
            .store(is_lead, Ordering::Release);
    }

    pub fn is_leader(&self) -> bool {
        self.process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .is_leader
            .load(Ordering::Acquire)
    }

    /// 设置Trap上下文
    pub fn set_trap_context(&self, trap_frame: TrapFrame) {
        let now_trap_frame = self
            .process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .trap_frame
            .get();
        unsafe {
            *now_trap_frame = trap_frame;
        }
    }
    /// 将trap上下文直接写入到内核栈上
    /// 注意此时保持sp不变
    /// 返回值为压入了trap之后的内核栈的栈顶，可以用于多层trap压入
    pub fn set_trap_in_kernel_stack(&self) {
        extern "C" {
            pub fn __copy(frame_address: *mut TrapFrame, kernel_base: usize);
        }
        let trap_frame_size = core::mem::size_of::<TrapFrame>();
        let frame_address = self
            .process_inner.as_ref()
            .unwrap_or_else(|| panic!("not a process"))
            .trap_frame
            .get();
        let kernel_base = self.get_kernel_stack_top().unwrap() - trap_frame_size;
        unsafe {
            __copy(frame_address, kernel_base);
        }
    }
}

impl TaskInner {
    pub fn new_vcpu<F>(
        entry: F,
        name: String,
        stack_size: usize,
        _vcpu_id: u64,
        _page_table_token: usize,
    ) -> AxTaskRef
    where
        F: FnOnce() + Send + 'static,
    {
        let mut t = Self::new_common(TaskId::new(), name, TaskType::Vcpu);
        debug!("new task: {}", t.id_name());
        let kstack = TaskStack::alloc(align_up_4k(stack_size));
        t.entry = Some(Box::into_raw(Box::new(entry)));
        t.ctx.get_mut().init(task_entry as usize, kstack.top());
        t.kstack = Some(kstack);
        if t.name == "idle" {
            t.is_idle = true;
        }
        Arc::new(AxTask::new(t))
    }
}

impl fmt::Debug for TaskInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("TaskInner")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("state", &self.state())
            .finish()
    }
}

impl Drop for TaskInner {
    fn drop(&mut self) {
        debug!("task drop: {}", self.id_name());
    }
}

struct TaskStack {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl TaskStack {
    pub fn alloc(size: usize) -> Self {
        let layout = Layout::from_size_align(size, 16).unwrap();
        Self {
            ptr: NonNull::new(unsafe { alloc::alloc::alloc(layout) }).unwrap(),
            layout,
        }
    }

    pub const fn top(&self) -> VirtAddr {
        unsafe { core::mem::transmute(self.ptr.as_ptr().add(self.layout.size())) }
    }

    #[cfg(feature = "monolithic")]
    /// 获取内核栈第一个压入的trap上下文，防止出现内核trap嵌套
    pub fn get_first_trap_frame(&self) -> *mut TrapFrame {
        (self.top().as_usize() - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame
    }
}

impl Drop for TaskStack {
    fn drop(&mut self) {
        unsafe { alloc::alloc::dealloc(self.ptr.as_ptr(), self.layout) }
    }
}

use core::mem::ManuallyDrop;

/// A wrapper of [`AxTaskRef`] as the current task.
pub struct CurrentTask(ManuallyDrop<AxTaskRef>);

impl CurrentTask {
    pub(crate) fn try_get() -> Option<Self> {
        let ptr: *const super::AxTask = axhal::cpu::current_task_ptr();
        if !ptr.is_null() {
            Some(Self(unsafe { ManuallyDrop::new(AxTaskRef::from_raw(ptr)) }))
        } else {
            None
        }
    }

    pub(crate) fn get() -> Self {
        Self::try_get().expect("current task is uninitialized")
    }

    /// Converts [`CurrentTask`] to [`AxTaskRef`].
    pub fn as_task_ref(&self) -> &AxTaskRef {
        &self.0
    }

    pub(crate) fn clone(&self) -> AxTaskRef {
        self.0.deref().clone()
    }

    pub(crate) fn ptr_eq(&self, other: &AxTaskRef) -> bool {
        Arc::ptr_eq(&self.0, other)
    }

    pub(crate) unsafe fn init_current(init_task: AxTaskRef) {
        let ptr = Arc::into_raw(init_task);
        axhal::cpu::set_current_task_ptr(ptr);
    }

    pub(crate) unsafe fn set_current(prev: Self, next: AxTaskRef) {
        let Self(arc) = prev;
        ManuallyDrop::into_inner(arc); // `call Arc::drop()` to decrease prev task reference count.
        let ptr = Arc::into_raw(next);
        axhal::cpu::set_current_task_ptr(ptr);
    }
}

impl Deref for CurrentTask {
    type Target = TaskInner;
    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

extern "C" fn task_entry() -> ! {
    // release the lock that was implicitly held across the reschedule
    unsafe { crate::RUN_QUEUE.force_unlock() };
    #[cfg(feature = "irq")]
    axhal::arch::enable_irqs();
    let task = crate::current();
    if let Some(entry) = task.entry {
        unsafe { Box::from_raw(entry)() };
    }
    crate::exit(0);
}
