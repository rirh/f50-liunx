import type { Messages } from "../zh";

const m: Messages["tools"] = {
  at: {
    page: {
      title: "AT terminal",
      description:
        "Send AT commands straight to the modem. PINs and passwords are not kept in the history.",
    },
    class: {
      scan: {
        title: "Network scan",
        detail:
          "Takes minutes and holds the control channel the whole time; it runs as a background job (up to 3 minutes).",
      },
      nv: {
        title: "Write NV settings",
        detail:
          "Changes settings the modem keeps in NV (network mode, NR mode, EN-DC, band or cell lock); they stay after a restart.",
      },
      radio: {
        title: "Radio / connection",
        detail:
          "Affects the radio, the data connection or the modem itself (CFUN, SFUN, CGATT, CGACT, COPS=n, ATD…); the connection may drop.",
      },
      apn: {
        title: "Data profile",
        detail:
          "Changes the saved data profiles (CGDCONT, CGAUTH); internet access may be affected.",
      },
      sim: {
        title: "SIM PIN",
        detail:
          "Touches the SIM PIN (CLCK, CPWD, CPIN); too many wrong PINs lock the SIM card.",
      },
    },
    error: {
      ascii:
        "An AT command is one line of printable ASCII starting with AT, at most 512 bytes",
      chaining: "Chaining commands with “;” is not allowed",
      wedges: "Refused: this command hangs the AT port until a restart",
      simSelection:
        "Refused: this command switches the SIM card or the data route; use the SIM page",
      timeout:
        "The modem did not answer in time; the command may still be running, so check before resending",
      busy: "A long AT command is still running in the background (job {job}); wait for it to finish",
    },
    form: {
      title: "Command",
      description:
        "One command at a time, sent on the control channel. Commands that change settings need confirming; scans and commands with a timeout over 15 seconds run as background jobs.",
      label: "AT command",
      send: "Send",
      sending: "Sending",
      history: "browse this boot's command history ({count} commands)",
      history_one: "browse this boot's command history ({count} command)",
      historyLoading: "browse this boot's command history (loading)",
      timeout: "Answer timeout",
      seconds: "s",
      quick: "Common commands",
      jobTitle: "Background command {command}",
      commandRequired: "Enter an AT command",
      commandMax: "At most {max} characters",
      commandAscii: "Printable ASCII characters only",
      commandPrefix: "The command must start with AT",
      timeoutRange: "1–60 seconds; leave empty for the default 8",
    },
    log: {
      resend: "Confirmed, sending again",
      jobStarted: "Long command, moved to background job {job}",
      needsConfirm: "Needs confirming before it is sent ({reason})",
      refused: "Refused: {message}",
      jobCancelled: "Background job cancelled",
      jobUnfinished: "Background job did not finish",
      sendCancelled: "Sending cancelled",
    },
    confirm: {
      title: "Confirmation needed",
      send: "Send anyway",
    },
    output: {
      title: "Output",
      count: "{count} entries",
      count_one: "{count} entry",
      empty: "No output yet",
      copied: "Output copied",
      copyFailed: "Could not copy",
      emptyTitle: "No commands yet",
      emptyDescription: "Type an AT command below, for example AT+CSQ.",
    },
  },
  jobs: {
    page: {
      title: "Background jobs",
      description:
        "Long operations such as sending SMS, long AT commands and updates run in the background; they are kept until the device restarts.",
    },
    list: {
      title: "Jobs",
      running: "{count} running",
      running_one: "{count} running",
      runningDescription: "Jobs are running; refreshing every 3 seconds.",
      idleDescription: "Click a row to see its state, result and log.",
      emptyTitle: "No jobs",
      emptyDescription: "No background jobs since this boot.",
    },
    kind: {
      at: "AT command",
      sms: "SMS",
      update: "System update",
      wifi: "Wi-Fi client",
      radio: "Radio",
    },
    method: {
      atSend: "Send AT command",
      smsSend: "Send SMS",
      updateApply: "Install update",
      updateApplyFile: "Install uploaded firmware",
      updateCheck: "Check for updates",
      updateRollback: "Roll back",
      updateUploadDone: "Verify uploaded firmware",
      wifiJoin: "Join Wi-Fi",
      wifiLeave: "Leave Wi-Fi",
      wifiScan: "Scan Wi-Fi",
    },
    cancel: {
      requested: "Asked the job to stop",
      title: "Stop job",
      description: "Stop “{name}” ({id})? Work already done is not undone.",
    },
    field: {
      action: "Action",
      method: "Method",
      kind: "Type",
      state: "State",
      cancellable: "Can stop",
      started: "Started",
      finished: "Finished",
      duration: "Duration",
    },
    detail: {
      title: "Job details",
      inProgress: "In progress",
      error: "Error ({code})",
      result: "Result",
      log: "Log (tail)",
      noLog: "No log",
    },
    table: {
      notCancellable: "Cannot stop",
    },
  },
};
export default m;
