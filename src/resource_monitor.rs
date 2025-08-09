use std::{cmp::Ordering, collections::HashMap, ffi::OsString};

use cosmic::iced::{self, alignment::Horizontal, Length, Padding};
use itertools::Itertools;
use nvml_wrapper::{enum_wrappers::device::Clock, error::NvmlError, Nvml};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use cosmic::iced_widget::{column, container, text, row, horizontal_rule, scrollable, Column, Text};
use crate::{shader::FragmentShaderProgram, App, Message};

const MAX_CPU_FREQ:f32 = 5500.;
const GRAPH_CHAR_WIDTH:usize = 28;
const GRAPH_GLYPHS : [char; 9] = [' ','▁','▂','▃','▄','▅','▆','▇','█'];


fn byte_to_gb(x:u64)->f32{(x/(1_000_000)) as f32/1000.}
fn byte_to_mb(x:u64)->u64{x/1_000_000}
fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

#[derive(Clone)]
pub struct CpuInfo{
    physical_cores:usize,
    cpu_count:usize,
    cpu_avg:f32,
    cpu_max:f32,
    cpu_freq:f32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct GpuInfo{
    mem_used:u64,
    mem_total:u64,
    clock:f32,
    power:f32,
    util:f32,
}

#[derive(Default)]
pub struct InterpolatedInfo{
    cpu_avg:f32,
    cpu_max:f32,
    cpu_freq:f32,
    cpu_avg_smooth:f32,
    cpu_freq_smooth:f32,
    cpu_max_smooth:f32,
    gpu_clock:f32,
    gpu_power:f32,
    gpu_util:f32,
}


#[derive(Default)]
pub struct ProcessInfo{
    name:OsString,
    cpu:f32,
    mem:u64,
    pid:u32,
}
impl ToString for ProcessInfo {
    fn to_string(&self) -> String {
        let cpu = format!("{:.1}", self.cpu);
        let cpu = if cpu.len() <= 3 {cpu} else {format!("{:3.0}", self.cpu)};
        format!(
            "{:^15}|{}% {:4}MB", 
            truncate(self.name.to_str().unwrap_or_default(), 15), 
            cpu, 
            byte_to_mb(self.mem),
        )
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub enum ProcessBy {
    #[default] Cpu,
    Ram,
}
impl ProcessBy {
    pub fn compare(self, a:&ProcessInfo, b:&ProcessInfo)->Ordering{
        match self{
            ProcessBy::Cpu => b.cpu.partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessBy::Ram => b.mem.partial_cmp(&a.mem)
                .unwrap_or(std::cmp::Ordering::Equal),
        }
    }
}

pub struct ResourceMonitor {
    // INTERNAL
    sys:System,
    refreshkind:RefreshKind,
    nv:Option<Nvml>,

    // GENERAL INFO
    cpu_name: String,
    architecture: String,
    os_name: String,
    kernel_name: String,
    os_version: String,
    gpu_name: String,
    mem_total:u64,

    // UPDATED INFO
    cpu_info: CpuInfo,
    gpu_info: GpuInfo,
    smooth:InterpolatedInfo,
    process_info: Vec<ProcessInfo>,
    process_sort_by:ProcessBy,
    ram_used:u64,

    // HISTORY
    cpu_avgs: [f32; GRAPH_CHAR_WIDTH],
    gpu_avgs: [f32; GRAPH_CHAR_WIDTH],
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
        let nv_init = Nvml::init();
        let nv = if let Ok(nv) = nv_init {
            Some(nv)
        } else {
            println!("ERROR INITIALIZING NVML: \n{:?}", nv_init);
            None
        };

        // collect information that need only be fetched once
        let cpu_name = sys.cpus().first().map(|cpu|(
            cpu.brand().split(" ").last().unwrap_or_default().to_owned()
        )).unwrap_or_default();

        let cpu_info = CpuInfo{ 
            physical_cores: sys.physical_core_count().unwrap_or_default(), 
            cpu_count: sys.cpus().len(), 
            cpu_avg: 0.,
            cpu_max: 0.,
            cpu_freq: 0., 
        };
        let mem_total = sys.total_memory();
        let gpu_name = gpu_name(&nv).ok().unwrap_or_default();

        Self { 
            sys: sys, 
            refreshkind: refreshkind,
            cpu_info: cpu_info.clone(),
            os_name: System::name().unwrap_or_default(),
            kernel_name: System::kernel_version().unwrap_or_default(),
            os_version: System::os_version().unwrap_or_default(),
            ram_used: 0,
            mem_total: mem_total,
            nv: nv,
            gpu_name,
            gpu_info: GpuInfo::default(),
            smooth: InterpolatedInfo{..Default::default()},
            cpu_name: cpu_name,
            architecture: System::cpu_arch(),
            process_info: vec![],
            process_sort_by: ProcessBy::default(),
            cpu_avgs: [0.0; GRAPH_CHAR_WIDTH],
            gpu_avgs: [0.0; GRAPH_CHAR_WIDTH],
        }
    }

    pub fn set_process_sorting(&mut self, sort_by:ProcessBy){
        self.process_sort_by = sort_by
    }

    pub fn update_cpu_gpu_mem(&mut self){
        // CPU
        self.sys.refresh_specifics(self.refreshkind);

        let cpu_avg = self.sys.global_cpu_usage();
        self.cpu_info = CpuInfo {
            cpu_avg: cpu_avg,
            cpu_max: self.sys.cpus().iter()
                .map(|cpu|cpu.cpu_usage())
                .fold(f32::NEG_INFINITY, |a, b| a.max(b)),
            cpu_freq: self.sys.cpus().iter()
                .map(|cpu|{cpu.frequency()})
                .sum::<u64>() as f32 / self.cpu_info.cpu_count as f32,
            ..self.cpu_info
        };
        
        // MEMORY
        self.ram_used = self.sys.used_memory();

        // GPU
        let gpudat = gpu_update(&self.nv).ok();
        self.gpu_info = gpudat.unwrap_or(self.gpu_info);

        // GRAPHS
        self.cpu_avgs.rotate_right(1);
        self.cpu_avgs[0] = cpu_avg;
        if let Some(gpudat) = gpudat{
            self.gpu_avgs.rotate_right(1);
            self.gpu_avgs[0] = gpudat.util;
        }
    }

    pub fn update_processes(&mut self){
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_cpu(),
        );

        let mut processes: HashMap<OsString, ProcessInfo> = HashMap::new();
        for (pid, process) in self.sys.processes(){
            let pi = ProcessInfo{
                name: process.name().to_owned(),
                cpu: process.cpu_usage(),
                mem: process.memory(),
                pid: pid.as_u32(),
            };
            if let Some(pi_old) = processes.get(&pi.name){
                processes.insert(pi.name, ProcessInfo{
                    name: pi_old.name.clone(),
                    pid: (*pi_old).pid,
                    cpu: f32::max(pi.cpu , (*pi_old).cpu),
                    mem: u64::max(pi.mem , (*pi_old).mem),
                });
            } else {
                processes.insert(pi.name.clone(), pi);
            }
        }

        self.process_info = processes.into_values()
            .sorted_by(|a,b| self.process_sort_by.compare(a, b))
            .collect::<Vec<ProcessInfo>>();
    }


    pub fn update_visual(&mut self, frag:&mut FragmentShaderProgram){
        const ALPHA:f32 = 0.95;
        const ALPHA_SMOOTH:f32 = 0.99;

        let to = |from:f32, to:f32| {
            ALPHA * from + (1.-ALPHA) * to
        };
        let to_smooth = |from:f32, to:f32| {
            ALPHA_SMOOTH * from + (1.-ALPHA_SMOOTH) * to
        };

        self.smooth = InterpolatedInfo{
            cpu_avg: to(self.smooth.cpu_avg, self.cpu_info.cpu_avg),
            cpu_max: to(self.smooth.cpu_max, self.cpu_info.cpu_max),
            cpu_freq: to(self.smooth.cpu_freq, self.cpu_info.cpu_freq),
            cpu_avg_smooth:  to_smooth(self.smooth.cpu_avg_smooth, self.cpu_info.cpu_avg),
            cpu_freq_smooth:  to_smooth(self.smooth.cpu_freq_smooth, self.cpu_info.cpu_freq),
            cpu_max_smooth:  to_smooth(self.smooth.cpu_max_smooth, self.cpu_info.cpu_max),
            gpu_clock: to(self.smooth.gpu_clock, self.gpu_info.clock),
            gpu_power: to(self.smooth.gpu_power, self.gpu_info.power),
            gpu_util: to(self.smooth.gpu_util, self.gpu_info.util),
        };

        frag.update_uniforms_tick(
            (self.smooth.cpu_avg_smooth/100.).clamp(0.0, 1.0), 
            (self.smooth.cpu_max_smooth/100.).clamp(0.0, 1.0), 
            (self.smooth.cpu_freq_smooth/MAX_CPU_FREQ).clamp(0.0, 1.0)
        );
    }

    fn get_graph(data: &[f32])->String{
        data.iter().map(|v| {
            let fract = 0.01 * v.clamp(0., 100.) * GRAPH_GLYPHS.len() as f32; // 0 to len
            let index = (fract.round() as usize).clamp(0, GRAPH_GLYPHS.len() - 1);
            GRAPH_GLYPHS[index]
        }).collect()
    }

    pub fn view_monitor(&self, app:&App)->iced::widget::Column<'_, Message, cosmic::Theme>{
        let res: iced::widget::Column<'_, Message, cosmic::Theme> = column!(
            // CLOCK
            container(
                text(
                    format!("{}", app.current_time.format("%H : %M : %S"))
                ).size(30).width(Length::Fill).align_x(Horizontal::Center)
            ).padding(Padding{bottom:10., ..Default::default()}).width(Length::Fill),
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
                self.cpu_name,
                self.architecture,
                self.cpu_info.physical_cores,
                self.cpu_info.cpu_count,
            )),
            text(format!("CPU AVG   {:2.0} %\nCPU MAX   {:2.0} %\nCPU FRQ {:4} MHz", 
                self.smooth.cpu_avg,
                self.smooth.cpu_max,
                self.smooth.cpu_freq as u64,
            )),
            text(Self::get_graph(&self.cpu_avgs)),
            horizontal_rule(2),
            // MEMORY
            row![
                text("MEM USE "),
                text(format!("{:.1}/{:.1}",
                    byte_to_gb(self.ram_used),
                    byte_to_gb(self.mem_total),
                )),
                text("GB")
            ],
            horizontal_rule(2),
            // GPU
            text(format!("{}", self.gpu_name)),
            text(format!("GPU UTL   {:2.0} %", self.smooth.gpu_util)),
            text(format!("GPU FRQ {:4} MHz",self.smooth.gpu_clock as u64)),
            text(Self::get_graph(&self.gpu_avgs)),
            text(format!("GPU MEM {:3.1}/{:3.1} GB",
                byte_to_gb(self.gpu_info.mem_used),
                byte_to_gb(self.gpu_info.mem_total))),
            text(format!("GPU PWR  {:3.0} W", self.smooth.gpu_power/1000.)),
            horizontal_rule(2),
        ).padding(Padding{left:10.,right:10.,bottom:20.,..Default::default()});
        res
    }

    pub fn view_processes(&self)->cosmic::iced_widget::Column<'_, Message, cosmic::Theme, cosmic::Renderer>{
        
        let header =  row![
            Text::new("      NAME     |"),
            // cosmic::iced_widget::Button::new(text(match self.process_sort_by{
            //     ProcessBy::Cpu => ">CPU",
            //     ProcessBy::Ram => " CPU",
            // })),
            // button(text(match self.process_sort_by{
            //     ProcessBy::Cpu => " RAM",
            //     ProcessBy::Ram => ">RAM",
            // }))
            // .on_press(Message::ProcessSortBy(ProcessBy::Ram)),
            text(" CPU"),
            text("   RAM"),
        ];

        let mut column: Column<'_, Message, cosmic::Theme, cosmic::Renderer> = Column::new();
        for pi in &self.process_info {
            column = column.push(Text::new(pi.to_string()));
        }

        column![
            // header:
            header.width(Length::Fill).height(Length::Shrink)
                .padding(Padding{top:30., bottom:5., ..Default::default()}),
            // scrollable:
            container(scrollable(column).width(Length::Fill))
                .height(Length::FillPortion(4))
                .padding(Padding{bottom:30., ..Default::default()}),
        ]
        .width(Length::Fill).height(Length::Fill)
    }
}

// struct TextButtonStyle;

// impl button::StyleSheet for TextButtonStyle {
//     type Style = cosmic::Theme;

//     fn active(&self, _style: &Self::Style) -> button::Style {
//         button::Style {
//             background: None,
//             border_radius: 0.0.into(),
//             border_width: 0.0,
//             border_color: cosmic::iced::Color::TRANSPARENT,
//             text_color: cosmic::iced::Color::from_rgb(0.9, 0.9, 0.9),
//             ..Default::default()
//         }
//     }

//     fn hovered(&self, style: &Self::Style) -> button::Appearance {
//         self.active(style)
//     }

//     fn pressed(&self, style: &Self::Style) -> button::Appearance {
//         self.active(style)
//     }
// }


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
