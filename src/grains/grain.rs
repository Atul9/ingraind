use std::marker::PhantomData;

pub struct Grain<T> {
    module: Module,
    _type: PhantomData<T>,
}

impl<'code, 'module, T> Grain<T>
where
    T: EBPFGrain<'code>,
{
    pub fn attach_kprobes(&mut self, backends: &[BackendHandler]) -> Vec<Box<dyn EventHandler>> {
        use redbpf::ProgramKind::*;
        for prog in self
            .module
            .programs
            .iter_mut()
            .filter(|p| p.kind == Kprobe || p.kind == Kretprobe)
        {
            info!("Attached: {}, {:?}", prog.name, prog.kind);
            prog.attach_probe().unwrap();
        }

        self.bind_perf(backends)
    }

    pub fn attach_xdps(
        &mut self,
        iface: &str,
        backends: &[BackendHandler],
    ) -> Vec<Box<dyn EventHandler>> {
        use redbpf::ProgramKind::*;
        for prog in self.module.programs.iter_mut().filter(|p| p.kind == XDP) {
            info!("Attached: {}, {:?}", prog.name, prog.kind);
            prog.attach_xdp(iface).unwrap();
        }

        self.bind_perf(backends)
    }

    fn bind_perf(&mut self, backends: &[BackendHandler]) -> Vec<Box<dyn EventHandler>> {
        let online_cpus = cpus::get_online().unwrap();
        let mut output: Vec<Box<dyn EventHandler>> = vec![];
        for ref mut m in self.module.maps.iter_mut().filter(|m| m.kind == 4) {
            for cpuid in online_cpus.iter() {
                let pm = PerfMap::bind(m, -1, *cpuid, 16, -1, 0).unwrap();
                output.push(Box::new(PerfHandler {
                    name: m.name.clone(),
                    perfmap: pm,
                    callback: T::get_handler(m.name.as_str()),
                    backends: backends.to_vec(),
                }));
            }
        }

        output
    }

    pub fn attach_socketfilters(
        mut self,
        iface: &str,
        backends: &[BackendHandler],
    ) -> Vec<Box<dyn EventHandler>> {
        use redbpf::ProgramKind::*;
        self.module
            .programs
            .iter_mut()
            .filter(|p| p.kind == SocketFilter)
            .map(|prog| {
                info!("Attached: {}, {:?}", prog.name, prog.kind);
                let fd = prog.attach_socketfilter(iface).unwrap();
                Box::new(SocketHandler {
                    socket: unsafe { Socket::from_raw_fd(fd) },
                    backends: backends.to_vec(),
                    callback: T::get_handler(prog.name.as_str()),
                }) as Box<dyn EventHandler>
            })
            .collect()
    }
}

pub trait EBPFGrain<'code> {
    fn code() -> &'code [u8];
    fn get_handler(id: &str) -> EventCallback;

    fn load() -> Result<Grain<Self>>
    where
        Self: Sized,
    {
        let mut module = Module::parse(Self::code())?;
        for prog in module.programs.iter_mut() {
            prog.load(module.version, module.license.clone()).unwrap();
        }

        Ok(Grain {
            module,
            _type: PhantomData,
        })
    }
}
