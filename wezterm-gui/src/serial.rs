use crate::{
    async_run_terminal_gui, build_initial_mux, maybe_show_configuration_error_window,
    set_window_position, terminate_with_error,
};
use config::SerialDomain;
use mux::domain::{Domain, LocalDomain};
use mux::Mux;
use std::sync::Arc;
use wezterm_gui_subcommands::{SerialCommand, StartCommand};

async fn async_run_serial(opts: SerialCommand) -> anyhow::Result<()> {
    let serial_domain = SerialDomain {
        name: format!("Serial Port {}", opts.port),
        port: Some(opts.port.clone()),
        baud: Some(opts.baud as usize),
        data_bits: Some(opts.data_bits),
        parity: Some(opts.parity),
        stop_bits: Some(opts.stop_bits),
        flow_control: Some(opts.flow_control),
    };

    let start_command = StartCommand {
        always_new_process: true,
        class: opts.class,
        cwd: None,
        no_auto_connect: true,
        position: opts.position,
        workspace: None,
        domain: Some(serial_domain.name.clone()),
        ..Default::default()
    };

    let cmd = None;

    let domain: Arc<dyn Domain> = Arc::new(LocalDomain::new_serial_domain(serial_domain)?);
    let mux = Mux::get();
    mux.add_domain(&domain);

    let should_publish = false;
    async_run_terminal_gui(cmd, start_command, should_publish).await
}

pub(crate) fn run(config: config::ConfigHandle, opts: SerialCommand) -> anyhow::Result<()> {
    if let Some(cls) = opts.class.as_ref() {
        crate::set_window_class(cls);
    }
    if let Some(pos) = opts.position.as_ref() {
        set_window_position(pos.clone());
    }

    build_initial_mux(&config, None, None)?;

    let gui = crate::frontend::try_new()?;

    promise::spawn::spawn(async {
        if let Err(err) = async_run_serial(opts).await {
            terminate_with_error(err);
        }
    })
    .detach();

    maybe_show_configuration_error_window();
    gui.run_forever()
}
