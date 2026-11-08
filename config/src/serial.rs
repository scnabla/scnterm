use crate::config::validate_domain_name;
use portable_pty::serial::{CharSize, FlowControl, Parity, SerialTty, StopBits};
use std::convert::TryFrom;
use std::str::FromStr;
use wezterm_dynamic::{FromDynamic, ToDynamic};

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

impl FromStr for SerialParity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "odd" => Ok(Self::Odd),
            "even" => Ok(Self::Even),
            _ => Err("校验位必须为 none、odd 或 even".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

impl FromStr for SerialFlowControl {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "software" => Ok(Self::Software),
            "hardware" => Ok(Self::Hardware),
            _ => Err("流控必须为 none、software 或 hardware".to_string()),
        }
    }
}

#[derive(Default, Debug, Clone, FromDynamic, ToDynamic)]
pub struct SerialDomain {
    /// The name of this specific domain.  Must be unique amongst
    /// all types of domain in the configuration file.
    #[dynamic(validate = "validate_domain_name")]
    pub name: String,

    /// Specifies the serial device name.
    /// On Windows systems this can be a name like `COM0`.
    /// On posix systems this will be something like `/dev/ttyUSB0`.
    /// If omitted, the name will be interpreted as the port.
    pub port: Option<String>,

    /// Set the baud rate.  The default is 9600 baud.
    #[dynamic(validate = "validate_baud")]
    pub baud: Option<usize>,

    /// 数据位：5、6、7 或 8；省略时为 8。
    #[dynamic(validate = "validate_data_bits")]
    pub data_bits: Option<u8>,

    /// 校验位：None、Odd 或 Even；省略时为 None。
    pub parity: Option<SerialParity>,

    /// 停止位：1 或 2；省略时为 1。
    #[dynamic(validate = "validate_stop_bits")]
    pub stop_bits: Option<u8>,

    /// 流控：None、Software 或 Hardware。省略时保留原有软件流控。
    pub flow_control: Option<SerialFlowControl>,

    // scnterm: 仅控制新连接是否自动记录；旧配置默认不启动日志。
    #[dynamic(default)]
    pub log_on_connect: bool,
}

fn validate_baud(value: &Option<usize>) -> Result<(), String> {
    if let Some(value) = value {
        if *value == 0 || u32::try_from(*value).is_err() {
            return Err("波特率必须为 1 至 4294967295 的整数".to_string());
        }
    }
    Ok(())
}

fn validate_data_bits(value: &Option<u8>) -> Result<(), String> {
    if matches!(value, Some(value) if !(5..=8).contains(value)) {
        return Err("数据位必须为 5、6、7 或 8".to_string());
    }
    Ok(())
}

fn validate_stop_bits(value: &Option<u8>) -> Result<(), String> {
    if matches!(value, Some(value) if !(1..=2).contains(value)) {
        return Err("停止位必须为 1 或 2".to_string());
    }
    Ok(())
}

impl SerialDomain {
    /// 在打开设备之前验证参数；Lua 与命令行使用同一套设置。
    pub fn serial_tty(&self) -> anyhow::Result<SerialTty> {
        validate_domain_name(&self.name).map_err(anyhow::Error::msg)?;
        validate_baud(&self.baud).map_err(anyhow::Error::msg)?;
        validate_data_bits(&self.data_bits).map_err(anyhow::Error::msg)?;
        validate_stop_bits(&self.stop_bits).map_err(anyhow::Error::msg)?;

        let port = self.port.as_deref().unwrap_or(&self.name);
        anyhow::ensure!(!port.trim().is_empty(), "串口名称不能为空");
        let mut serial = SerialTty::new(port);
        if let Some(baud) = self.baud {
            serial.set_baud_rate(u32::try_from(baud)?);
        }
        if let Some(bits) = self.data_bits {
            serial.set_char_size(match bits {
                5 => CharSize::Bits5,
                6 => CharSize::Bits6,
                7 => CharSize::Bits7,
                8 => CharSize::Bits8,
                _ => unreachable!("数据位已验证"),
            });
        }
        if let Some(parity) = self.parity {
            serial.set_parity(match parity {
                SerialParity::None => Parity::None,
                SerialParity::Odd => Parity::Odd,
                SerialParity::Even => Parity::Even,
            });
        }
        if let Some(bits) = self.stop_bits {
            serial.set_stop_bits(match bits {
                1 => StopBits::One,
                2 => StopBits::Two,
                _ => unreachable!("停止位已验证"),
            });
        }
        if let Some(flow_control) = self.flow_control {
            serial.set_flow_control(match flow_control {
                SerialFlowControl::None => FlowControl::None,
                SerialFlowControl::Software => FlowControl::XonXoff,
                SerialFlowControl::Hardware => FlowControl::RtsCts,
            });
        }
        Ok(serial)
    }
}
