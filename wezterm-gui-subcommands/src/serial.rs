use clap::Parser;
use config::{GuiPosition, SerialFlowControl, SerialParity};

#[derive(Debug, Parser, Clone)]
pub struct SerialCommand {
    /// 波特率，默认 115200。
    #[arg(long, default_value = "115200", value_parser = clap::value_parser!(u32).range(1..))]
    pub baud: u32,

    /// 数据位：5、6、7 或 8。
    #[arg(long, default_value = "8", value_parser = clap::value_parser!(u8).range(5..=8))]
    pub data_bits: u8,

    /// 校验位：none、odd 或 even。
    #[arg(long, default_value = "none")]
    pub parity: SerialParity,

    /// 停止位：1 或 2。
    #[arg(long, default_value = "1", value_parser = clap::value_parser!(u8).range(1..=2))]
    pub stop_bits: u8,

    /// 流控：none、software（XON/XOFF）或 hardware（RTS/CTS）。
    #[arg(long, default_value = "none")]
    pub flow_control: SerialFlowControl,

    /// Override the default windowing system class.
    /// The default is "org.wezfurlong.wezterm".
    /// Under X11 and Windows this changes the window class.
    /// Under Wayland this changes the app_id.
    /// This changes the class for all windows spawned by this
    /// instance of wezterm, including error, update and ssh
    /// authentication dialogs.
    #[arg(long = "class")]
    pub class: Option<String>,
    /// Override the position for the initial window launched by this process.
    ///
    /// --position 10,20          to set x=10, y=20 in screen coordinates
    /// --position screen:10,20   to set x=10, y=20 in screen coordinates
    /// --position main:10,20     to set x=10, y=20 relative to the main monitor
    /// --position active:10,20   to set x=10, y=20 relative to the active monitor
    /// --position HDMI-1:10,20   to set x=10, y=20 relative to the monitor named HDMI-1
    #[arg(long, verbatim_doc_comment)]
    pub position: Option<GuiPosition>,

    /// Specifies the serial device name.
    /// On Windows systems this can be a name like `COM0`.
    /// On posix systems this will be something like `/dev/ttyUSB0`
    #[arg(value_parser)]
    pub port: String,
}
