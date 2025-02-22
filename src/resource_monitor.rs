use cosmic::{iced::{self, Length, Padding}, iced_widget::horizontal_rule};
use nvml_wrapper::{enum_wrappers::device::Clock, error::NvmlError, Nvml};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

use cosmic::iced_widget::{column, container, text};

use crate::{App, Message};

pub struct CpuInfo{
    name:String,
    physical_cores:usize,
    cpu_count:usize,
    architecture:String,
}

#[derive(Default, Clone, Copy)]
pub struct GpuInfo{
    mem_used:u64,
    mem_total:u64,
    clock:f32,
    power:f32,
    util:f32,
}

pub struct ResourceMonitor {
    // INTERNAL
    sys:System,
    refreshkind:RefreshKind,
    nv:Option<Nvml>,

    // ONE-TIME FETCH
    pub cpu_info: CpuInfo,
    pub os_name: String,
    pub kernel_name: String,
    pub os_version: String,
    pub mem_total:u64,
    pub gpu_name: String,

    // REFRESH FREQUENTLY
    pub gpu_info: GpuInfo,
    pub gpu_info_cur: GpuInfo,
    cpu_avg_cur:f32,
    pub cpu_avg:f32,
    cpu_max_cur:f32,
    pub cpu_max:f32,
    cpu_freq_cur:f32,
    pub cpu_freq:f32,
    pub mem_used:u64,
}

impl ResourceMonitor{
    pub fn new()->Self{
        // set up sysinfo
        let refreshkind = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::nothing().with_ram());
        let mut sys = System::new_with_specifics(refreshkind);
        sys.refresh_specifics(refreshkind);

        // set up nvml
        let nv = Nvml::init().ok();

        // collect information that need only be fetched once
        let cpu_info = CpuInfo{ 
            name: sys.cpus().first().map(|cpu|(
                cpu.brand().split(" ").last().unwrap_or_default().to_owned()
            )).unwrap_or_default(), 
            physical_cores: sys.physical_core_count().unwrap_or_default(), 
            cpu_count: sys.cpus().len(), 
            architecture: System::cpu_arch() 
        };
        let mem_total = sys.total_memory();
        let gpu_name = gpu_name(&nv).ok().unwrap_or_default();

        Self { 
            sys: sys, 
            refreshkind: refreshkind,
            cpu_info: cpu_info,
            cpu_avg: 0., 
            cpu_avg_cur: 0.,
            cpu_max: 0.,
            cpu_max_cur:0.,
            os_name: System::name().unwrap_or_default(),
            kernel_name: System::kernel_version().unwrap_or_default(),
            os_version: System::os_version().unwrap_or_default(),
            cpu_freq: 0.,
            cpu_freq_cur: 0.,
            mem_used: 0,
            mem_total: mem_total,
            nv: nv,
            gpu_info: GpuInfo::default(),
            gpu_name,
            gpu_info_cur: GpuInfo::default(),
        }
    }

    pub fn update_data(&mut self){
        // CPU
        self.sys.refresh_specifics(self.refreshkind);
        self.cpu_avg_cur = self.sys.global_cpu_usage();
        self.cpu_max_cur = self.sys.cpus().iter()
            .map(|cpu|cpu.cpu_usage())
            .fold(f32::NEG_INFINITY, |a, b| a.max(b));
        self.cpu_freq = self.sys.cpus().iter()
            .map(|cpu|{cpu.frequency()})
            .sum::<u64>() as f32 / self.cpu_info.cpu_count as f32;
        // MEMORY
        self.mem_used = self.sys.used_memory();
        // GPU
        self.gpu_info_cur = gpu_update(&self.nv).ok().unwrap_or(self.gpu_info_cur)
    }

    pub fn update_visual(&mut self){
        let towards = |from:f32, to:f32| {
            const ALPHA:f32 = 0.95;
            ALPHA * from + (1.-ALPHA) * to
        };
        self.cpu_avg = towards(self.cpu_avg, self.cpu_avg_cur);
        self.cpu_max = towards(self.cpu_max, self.cpu_max_cur);
        self.cpu_freq = towards(self.cpu_freq, self.cpu_freq_cur);
        self.gpu_info = GpuInfo{
            mem_used: self.gpu_info_cur.mem_used,
            mem_total: self.gpu_info_cur.mem_total,
            clock: towards(self.gpu_info.clock, self.gpu_info_cur.clock),
            power: towards(self.gpu_info.power, self.gpu_info_cur.power),
            util: towards(self.gpu_info.util, self.gpu_info_cur.util),
        }
    }

    pub fn get_monitor(&self, app:&App)->iced::widget::Column<'_, Message, cosmic::Theme>{
        let byte_to_gb = |x:u64| {(x/(1_000_000)) as f32/1000.};
        column!(
            // CLOCK
            container(text(format!("{}", 
            app.current_time.format("%H:%M:%S")
            )).size(30).width(Length::Fill)
            ).padding(Padding{bottom:10., ..Default::default()}),
            horizontal_rule(2),
            // SYSTEM
            text(format!("OS {} {} \nKERNEL {}\n", 
                self.os_name,
                self.os_version,
                self.kernel_name,
            )),
            horizontal_rule(2),
            // CPU
            text(format!("{} {} @{}C/{}T", 
                self.cpu_info.name,
                self.cpu_info.architecture,
                self.cpu_info.physical_cores,
                self.cpu_info.cpu_count,
            )),
            text(format!("CPU AVG   {:2.0} %\nCPU MAX   {:2.0} %\nCPU FRQ {:4} MHz", 
                self.cpu_avg,
                self.cpu_max,
                self.cpu_freq as u64,
            )),
            horizontal_rule(2),
            // MEMORY
            text(format!("MEM USE {:.1}/{:.1} GB",
                byte_to_gb(self.mem_used),
                byte_to_gb(self.mem_total),
            )),
            horizontal_rule(2),
            // GPU
            text(format!("{}", self.gpu_name)),
            text(format!("GPU UTL   {:2.0} %\nGPU FRQ {:4} MHz\nGPU MEM {:3.1}/{:3.1} GB\nGPU PWR  {:3.0} W", 
                self.gpu_info.util,
                self.gpu_info.clock as u64,
                byte_to_gb(self.gpu_info.mem_used),
                byte_to_gb(self.gpu_info.mem_total),
                self.gpu_info.power/1000.,
            )),
            horizontal_rule(2),
        ).padding(Padding{left:10.,right:10.,..Default::default()})
    }
}


fn gpu_name(nv:& Option<Nvml>)-> Result<String, NvmlError>{
    if let Some(nv) = nv{
        let device = nv.device_by_index(0)?;
        Ok(device.name()?)
    } else {Err(NvmlError::NoData)}
}

fn gpu_update(nv:& Option<Nvml>)-> Result<GpuInfo, NvmlError>{
    if let Some(nv) = nv{
        let device = nv.device_by_index(0)?;
        let mem = device.memory_info()?;
        let clock = device.clock_info(Clock::Graphics)?;
        let utilization = device.utilization_rates()?;
        let power = device.power_usage()?;
        Ok(GpuInfo { 
            mem_used: mem.used,
            mem_total: mem.total,
            clock: clock as f32,
            power: power as f32,
            util: utilization.gpu as f32
        })
    } else {Err(NvmlError::NoData)}
}
