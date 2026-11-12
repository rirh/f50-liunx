/** `tools` messages: the AT terminal and background jobs. */
export default {
  at: {
    page: {
      title: "AT 终端",
      description: "直接向调制解调器发送 AT 命令。PIN 和密码不会写入历史记录。",
    },
    class: {
      scan: {
        title: "网络扫描",
        detail:
          "耗时数分钟并一直占用控制通道，将作为后台任务运行（最长 3 分钟）。",
      },
      nv: {
        title: "写入 NV 设置",
        detail:
          "修改调制解调器保存在 NV 中的设置（网络模式、NR 模式、EN-DC、频段或小区锁定），重启后仍然生效。",
      },
      radio: {
        title: "射频 / 连接",
        detail:
          "影响射频、数据连接或调制解调器本身（CFUN、SFUN、CGATT、CGACT、COPS=n、ATD 等），可能导致断网。",
      },
      apn: {
        title: "数据配置",
        detail: "修改保存的数据配置（CGDCONT、CGAUTH），可能影响上网。",
      },
      sim: {
        title: "SIM 卡 PIN",
        detail:
          "涉及 SIM 卡 PIN（CLCK、CPWD、CPIN）；多次输错 PIN 会锁住 SIM 卡。",
      },
    },
    error: {
      ascii: "AT 命令须为一行以 AT 开头的可打印 ASCII 字符，最多 512 字节",
      chaining: "不允许用“;”串联多条命令",
      wedges: "已拒绝：该命令会让 AT 端口卡死，直到重启",
      simSelection: "已拒绝：该命令会切换 SIM 卡或数据路由，请在 SIM 页面操作",
      timeout: "调制解调器未及时应答；命令可能仍在执行，请先确认再重发",
      busy: "后台仍有长时间 AT 命令在运行（任务 {job}），请等待它结束",
    },
    form: {
      title: "命令",
      description:
        "一次一条命令，在控制通道上发送。会改动设置的命令需要确认；扫描和超时超过 15 秒的命令以后台任务运行。",
      label: "AT 命令",
      send: "发送",
      sending: "发送中",
      history: "浏览本次开机的历史命令（{count} 条）",
      history_one: "浏览本次开机的历史命令（{count} 条）",
      historyLoading: "浏览本次开机的历史命令（读取中）",
      timeout: "应答超时",
      seconds: "秒",
      quick: "常用命令",
      jobTitle: "后台命令 {command}",
      commandRequired: "请输入 AT 命令",
      commandMax: "最多 {max} 个字符",
      commandAscii: "只能是可打印的 ASCII 字符",
      commandPrefix: "命令须以 AT 开头",
      timeoutRange: "1–60 秒，留空为默认 8 秒",
    },
    log: {
      resend: "已确认，重新发送",
      jobStarted: "长时间命令，已转为后台任务 {job}",
      needsConfirm: "需要确认后才能发送（{reason}）",
      refused: "已拒绝：{message}",
      jobCancelled: "后台任务已取消",
      jobUnfinished: "后台任务未完成",
      sendCancelled: "已取消发送",
    },
    confirm: {
      title: "需要确认",
      send: "确认发送",
    },
    output: {
      title: "输出",
      count: "{count} 条记录",
      count_one: "{count} 条记录",
      empty: "尚无输出",
      copied: "已复制输出",
      copyFailed: "复制失败",
      emptyTitle: "还没有命令",
      emptyDescription: "在下方输入 AT 命令，例如 AT+CSQ。",
    },
  },
  jobs: {
    page: {
      title: "后台任务",
      description:
        "发送短信、长时间 AT 命令、更新等耗时操作在后台运行；设备重启前保留。",
    },
    list: {
      title: "任务列表",
      running: "{count} 个进行中",
      running_one: "{count} 个进行中",
      runningDescription: "有任务在运行，每 3 秒自动刷新。",
      idleDescription: "点击一行查看状态、结果和日志。",
      emptyTitle: "没有任务",
      emptyDescription: "本次开机以来还没有后台任务。",
    },
    kind: {
      at: "AT 命令",
      sms: "短信",
      update: "系统更新",
      wifi: "Wi-Fi 客户端",
      radio: "射频",
    },
    method: {
      atSend: "发送 AT 命令",
      smsSend: "发送短信",
      updateApply: "安装更新",
      updateApplyFile: "安装上传的固件",
      updateCheck: "检查更新",
      updateRollback: "回滚",
      updateUploadDone: "校验上传的固件",
      wifiJoin: "连接 Wi-Fi",
      wifiLeave: "断开 Wi-Fi",
      wifiScan: "扫描 Wi-Fi",
    },
    cancel: {
      requested: "已请求停止任务",
      title: "停止任务",
      description: "停止“{name}”（{id}）？已完成的部分不会撤销。",
    },
    field: {
      action: "操作",
      method: "方法",
      kind: "类别",
      state: "状态",
      cancellable: "可停止",
      started: "开始",
      finished: "结束",
      duration: "用时",
    },
    detail: {
      title: "任务详情",
      inProgress: "进行中",
      error: "错误（{code}）",
      result: "结果",
      log: "日志（末尾）",
      noLog: "没有日志",
    },
    table: {
      notCancellable: "不可停止",
    },
  },
};
