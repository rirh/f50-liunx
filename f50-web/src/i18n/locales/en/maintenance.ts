import type { Messages } from "../zh";

const m: Messages["maintenance"] = {
  boot: {
    title: "Boot",
    description:
      "Boot slots, fallback policy and the last result of each boot stage",
    stages: "Boot stages",
    filter: {
      placeholder: "Filter steps or details",
      aria: "Filter boot steps",
    },
    summary: {
      title: "Boot control",
      description:
        "Whether the device keeps booting Linux or falls back to Android (read-only)",
      slot: "Linux slot",
      defaultBoot: "Default boot",
      linuxDefault: "Linux is re-armed after every good boot",
      androidDefault: "Every Linux boot is a one-shot trial",
      attempts: "Allowed failures",
      attemptsValue:
        "Falls back to Android after {count} failed boots in a row",
      attemptsValue_one: "Falls back to Android after {count} failed boot",
      locked: "Lock Linux",
      lockedOn: "Locked",
      lockedOff: "Not locked",
      lockedHint: "Stays on Linux whatever the boot result",
      sinceGood: "Boots since the last good one",
      writable: "Boot control block writable",
      writableOn: "Writable",
      writableOff: "Not writable",
    },
    misc: {
      title: "misc boot control block",
      description: "The A/B slot state the bootloader reads",
      unreadable: "Cannot read the misc partition.",
      magicOk: "Magic OK",
      magicMissing: "Magic missing",
      crcOk: "CRC OK",
      crcBad: "CRC error",
      slot: "Slot",
      priority: "Priority",
      tries: "Tries left",
      successful: "Booted successfully",
      raw: "Raw data",
    },
    stage: {
      early: { label: "Early (preinit)", description: "The early-boot logger" },
      vendor: {
        label: "Vendor (S09)",
        description: "The vendor daemons' chroot and modem control",
      },
      hardware: {
        label: "Hardware (S12)",
        description:
          "Module loading helpers, Wi-Fi regulatory and vendor drivers",
      },
      post: {
        label: "Finish (S99)",
        description: "Marks this boot as good after a delay",
      },
      noClock: "Clock not set",
      uptime: "{n} s after power-on",
      duration: "took {n} s",
      noReport: "No report",
      ok: "OK",
      okWithFailures: "OK ({count} non-fatal failures)",
      okWithFailures_one: "OK ({count} non-fatal failure)",
      noMatch: "No matching steps",
      noSteps: "No steps in this stage",
    },
    outcome: { done: "Done", skipped: "Skipped", failed: "Failed" },
    step: { name: "Step", outcome: "Result", detail: "Details" },
  },
  logs: {
    liveHint: "Refreshes as soon as the logs change (pushed live)",
    matched: "{count} matching lines",
    matched_one: "{count} matching line",
    title: "System log",
    description:
      "Kernel log (dmesg) and system log (logread), newest at the bottom",
    copied: "Copied {count} lines",
    copied_one: "Copied {count} line",
    copyFailed: "Copy failed: the browser does not allow clipboard access",
    empty: "The log is empty",
    noMatch: "No matching log lines",
    card: {
      title: "Logs",
      description: "The last {lines} lines of each log, up to 256 KiB",
      filtered: "; filtered, showing {count} lines",
      filtered_one: "; filtered, showing {count} line",
      reading: "Reading",
      auto: "Auto refresh",
    },
    toolbar: {
      filterPlaceholder: "Filter (case-insensitive)",
      filterAria: "Filter logs",
      linesAria: "Lines per log",
      lines: "Last {count} lines",
      lines_one: "Last {count} line",
      auto: "Refresh every 5 s",
    },
  },
};
export default m;
