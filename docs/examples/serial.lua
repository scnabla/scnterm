-- 将此文件放在主配置所在目录，文件名保留为 serial.lua。
-- COM3 是示例端口，使用时改为 Windows 设备管理器中的实际端口。
return {
  {
    name = 'board-console',
    port = 'COM3',
    baud = 115200,
    data_bits = 8,
    parity = 'None',
    stop_bits = 1,
    flow_control = 'None',
  },
}
