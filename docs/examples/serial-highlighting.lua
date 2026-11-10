-- ScnTerm 串口关键词高亮示例。
-- 已有主 Lua 配置时，只复制 config.serial_highlighting 段到 return config 前。
-- 串口连接参数继续使用主配置中的 config.serial_ports。
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.serial_highlighting = {
  rules = {
    {
      foreground = '#FF6B81',
      keywords = { '错误', '失败', '[故障]' },
      patterns = { [[\b(?:error|fail(?:ed|ure)?|fatal)\b]] },
    },
    {
      foreground = '#FFB52E',
      keywords = { '警告', '超时' },
      patterns = { [[\bwarn(?:ing)?\b]], [[\btime(?:d)?[ -]?out\b]] },
    },
  },
}

return config
