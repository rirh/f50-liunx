import type { Messages } from "../zh";

const m: Messages["network"] = {
  shared: {
    ssidHint:
      "1-32 bytes, no control characters, no space at either end, not starting with -",
    pskHint: "8-63 printable ASCII characters",
    noChanges: "Nothing to save",
    saveApply: "Save and apply",
    submitting: "Submitting",
    actions: "Actions",
    pair: "{label}: {value}",
    sep: "; ",
  },
  wifi: {
    title: "Wi-Fi hotspot",
    description: "Hotspot name, password, band and channel",
    restartAlert: {
      title: "Applying settings restarts Wi-Fi",
      description:
        "Wi-Fi reloads about 2 seconds after saving and every hotspot client is disconnected. If this browser is connected through the hotspot, be ready to reconnect with the new name or password.",
    },
    schema: {
      country: "Two capital letters, e.g. CN",
      channel24: "2.4 GHz channels are 1-13",
      channel5:
        "5 GHz channel {channel} is not allowed in this country (or is a DFS channel)",
      width24: "2.4 GHz allows up to 40 MHz",
      noBand5:
        "Country {country} has no non-DFS 5 GHz channel; switch to 2.4 GHz first",
    },
    security: {
      wpa2: "WPA2-PSK",
      "wpa2-wpa3": "WPA2/WPA3 mixed",
      wpa3: "WPA3-SAE",
      open: "Open (no password)",
      other: "Other",
    },
    status: {
      unnamed: "Unnamed hotspot",
      clientMode: "Radio used for uplink",
      description: "Current hotspot status",
      band: "Band",
      channel: "Channel",
      channelLive: " (now {channel})",
      mode: "Mode",
      country: "Country/region",
      security: "Security",
      password: "Password",
      set: "Set",
      unset: "Not set",
      hidden: "Hidden SSID",
      clients: "Connected devices",
    },
    power: {
      result: {
        pending: "Submitted; Wi-Fi is reloading",
        unchanged: "No change",
        saved: "Saved, but the reload failed; takes effect after a reboot",
      },
      offTitle: "Turn off the hotspot?",
      offDescription:
        "Every device connected over Wi-Fi will be disconnected. If you are using the console through the hotspot, you will lose access and need to connect over USB instead.",
      off: "Turn off hotspot",
      turningOff: "Turning off",
      on: "Turn on hotspot",
      turningOn: "Turning on",
      radioBusy:
        "The radio is in use for the Wi-Fi uplink; disconnect from that network first",
    },
    clients: {
      title: "Connected devices",
      description:
        "Stations reported by hostapd; addresses and names come from DHCP leases",
      name: "Name",
      ip: "IP address",
      mac: "MAC address",
      empty: "No devices connected to the hotspot",
    },
    settings: {
      title: "Hotspot settings",
      description:
        "Only changed fields are sent; leave the password empty to keep it.",
      arg: {
        ssid: "Name",
        password: "Password",
        band: "Band",
        channel: "Channel",
        width: "Width",
        hidden: "Hidden",
        country: "Country",
      },
      newPassword: "(new password)",
      saved: "Saved; Wi-Fi reloads in about 2 seconds",
      clientMode: {
        title: "The radio is in use for the Wi-Fi uplink",
        description:
          "Settings are saved now and take effect when the hotspot is back.",
      },
      noWpa3: {
        title: "This device does not support WPA3",
        description:
          "Saving any change switches the hotspot to WPA2-PSK (CCMP); enter a new password if the current one cannot be used.",
      },
      ssid: "Network name (SSID)",
      password: "Password",
      passwordSet: "Set; leave empty to keep",
      passwordUnset: "Not set (open network)",
      openToWpa2: "; setting a password on an open hotspot switches it to WPA2",
      band: "Band",
      country: "Country/region code",
      countryHint: "Decides the available 5 GHz channels ({channels})",
      channel: "Channel",
      channelUnavailable: "{channel} (unavailable)",
      channelHint:
        "DFS channels are not offered (the driver has no radar detection)",
      width: "Channel width",
      widthHint: "Up to the widest the chosen channel allows in this country",
      hidden: "Hide network name (do not broadcast SSID)",
      confirmTitle: "Apply Wi-Fi settings?",
      confirmDescription:
        "Wi-Fi reloads in about 2 seconds and hotspot clients drop briefly. If you are using the console through the hotspot, you may need to reconnect with the new settings.",
      ap5: {
        enterCode: "enter a two-letter code",
        list: "5 GHz: {channels}",
        none: "no 5 GHz channel available",
      },
    },
  },
  client: {
    title: "Wi-Fi uplink",
    description:
      "Use another Wi-Fi network as the uplink (preferred over mobile data)",
    joined: "Joined the network",
    left: "Disconnected; the hotspot is coming back",
    leaveJob: "Disconnect network",
    joinJob: "Join {ssid}",
    security: {
      wpa3Only:
        "WPA3 only: this device's Wi-Fi driver does not support WPA3, so it cannot join",
      unsupported: "{security} networks are not supported",
      label: {
        "WPA2/3": "WPA2/WPA3",
        WPA3: "WPA3",
        WPA2: "WPA2",
        WPA: "WPA",
        OWE: "OWE",
        EAP: "Enterprise EAP",
        WEP: "WEP",
        open: "Open",
      },
    },
    signal: {
      strong: "strong",
      fair: "fair",
      weak: "weak",
      text: "{dbm} dBm ({quality})",
    },
    status: {
      mode: { off: "Radio idle", client: "Wi-Fi uplink", hotspot: "Hotspot" },
      firewall: {
        none: "No rules",
        closed: "Closed (no forwarding)",
        shared: "Shared with the LAN",
        unknown: "Could not read",
      },
      connecting: "Connecting",
      radio: "Radio now: {mode}",
      signal: "Signal",
      address: "Address",
      firewallLabel: "Firewall",
      firewallMismatch: "Does not match state",
      saved: "Saved network",
      atBoot: "Joins at boot",
      notAtBoot: "Not joined at boot",
    },
    leave: {
      action: "Disconnect",
      title: "Disconnect the Wi-Fi uplink?",
      description:
        "The radio goes back to the hotspot and the device uses mobile data.",
      saved: "Saved network",
      disable: {
        label: "Keep",
        hint: "Keep the network but do not join it at boot",
      },
      keep: {
        label: "Keep and auto-join",
        hint: "Still join at boot (the hotspot comes back after this disconnect)",
      },
      forget: {
        label: "Forget",
        hint: "Delete the saved network and password",
      },
    },
    networks: {
      name: "Name",
      signal: "Signal",
      security: "Security",
      notJoinable: "Cannot join",
      join: "Join",
      empty:
        "No networks found (hidden networks are not listed; join one by typing its name)",
    },
    scan: {
      title: "Nearby networks",
      description:
        "The radio cannot scan while the hotspot runs; you can join by typing a network name (takes effect after a reboot).",
      manual: "Join manually",
      scan: "Scan",
      scanning: "Scanning",
      job: "Scan networks",
      emptyTitle: "Not scanned yet",
      emptyDescription:
        "Click “Scan” to list nearby networks (about 4-75 seconds).",
    },
    join: {
      title: "Join a Wi-Fi network",
      description:
        "Connect as the uplink and save it as the network to join at boot.",
      passwordRequired: "Enter a password, or tick open network",
      passwordInvalid: "8-63 printable ASCII characters, or a 64-digit hex key",
      hotspotOn: {
        title: "The hotspot is running",
        description:
          "This driver cannot switch the hotspot to client mode without a reboot: the network is saved and the device joins it after rebooting (the hotspot will not start). Browsers connected through the hotspot will be disconnected.",
      },
      ssid: "Network name (SSID)",
      open: "Open network (no password)",
      password: "Password",
      passwordHint:
        "WPA2 password (8-63 characters) or a 64-digit hex key; WPA3-only networks are not supported",
      submit: "Join",
    },
  },
  lan: {
    title: "LAN",
    description: "Router address, subnet and DHCP",
    moved: {
      title: "LAN address changed",
      description:
        "After reconnecting, open the console at the new address (you will need to sign in again):",
    },
    schema: {
      address:
        "Must be the .1 address of a private network, e.g. 192.168.0.1, 10.0.0.1, 172.16.0.1",
      netmask: "Unsupported subnet mask",
      dhcpStart: "A whole number from 2 to 254",
      dhcpLimit: "A whole number from 1 to 253",
      leasetime: "e.g. 12h, 30m or infinite (1-99999 minutes/hours)",
      poolOverflow: "Start offset + count cannot exceed 255",
    },
    settings: {
      title: "LAN settings",
      description:
        "The network and DHCP service reload about 2 seconds after saving.",
      saved: "Saved; takes effect in about 2 seconds",
      savedMoved: "Saved; the LAN moves to the new address in about 2 seconds",
      address: "Router address",
      addressHint:
        "The .1 address of a private network; afterwards the console is at the new address",
      netmask: "Subnet mask",
      dhcp: "Enable DHCP server",
      dhcpStart: "Start offset",
      dhcpLimit: "Address count",
      leasetime: "Lease time",
      leasetimeHint: "e.g. 12h, 30m, infinite",
      pool: "Address pool: ",
      confirm: {
        title: "Change the LAN address?",
        confirm: "Save and switch",
        description:
          "In about 2 seconds the LAN moves to the new network, and every connected device needs a new address (reconnect Wi-Fi or replug USB).",
        consoleAt: "The console moves to: ",
        newNetmask: "New subnet mask: ",
      },
    },
    status: {
      title: "Current LAN",
      description: "Router address and DHCP pool",
      address: "Router address",
      netmask: "Subnet mask",
      live: "br-lan actual address",
      pending: "Pending",
      usbHost: "USB host address",
      default: "Factory address",
      pool: "Address pool",
      leasetime: "Lease time",
    },
  },
  usb: {
    description: "USB-C port role, USB networking, USB adapters and adb",
    role: {
      host: "Host (USB adapters)",
      device: "Device (to a computer)",
      none: "None",
      unavailable: "Not supported",
    },
    netMode: {
      default: "Default (NCM)",
      ncm: "NCM",
      ecm: "ECM",
      rndis: "RNDIS",
      multi: "NCM + RNDIS",
    },
    netModeHint: {
      default: "NCM: driverless on Linux, macOS and recent Windows",
      ncm: "Driverless on Linux, macOS and Windows 11; fastest",
      ecm: "For older Linux and some macOS",
      rndis: "Older Windows",
      multi: "Offers both NCM and RNDIS",
    },
    netBound: {
      ncm: "NCM",
      ecm: "ECM",
      rndis: "RNDIS",
      multi: "NCM + RNDIS",
      mixed: "Mixed (abnormal)",
      none: "None",
    },
    adbState: {
      off: "Off",
      unavailable: "Unavailable",
      open: "Open (no authorization)",
      keys: "Authorized keys only",
    },
    status: {
      title: "USB-C port",
      description: "Role, USB networking and link speed",
      port: "Port",
      device: "Device",
      speed: "Speed",
      deviceFixed: "Device · fixed",
      roleBoot: "Role at boot",
      host: "Host",
      deviceDefault: "Device (default)",
      netCurrent: "Current network function",
      netNow: "Mode this boot",
      netNext: "Mode next boot",
      onceSuffix: " (once)",
      netName: "Interface name on computer",
      controller: "Controller",
      bound: "Bound",
      unbound: "Unbound",
      linkState: "Link state",
      linkSpeed: "Link speed",
      maxSpeed: " (max {speed})",
    },
    roleCard: {
      title: "Port role",
      description:
        "Device: connects to a computer as a USB network adapter. Host: takes USB Ethernet adapters (needs a powered hub).",
      saved: "Saved; the port is about to switch role",
      unsupported: {
        title: "Role switching not supported",
        description: "This kernel has no USB role switch interface.",
      },
      role: "Role",
      device: "Device",
      atBoot: "Use host mode at every boot",
      atBootHint:
        "Host mode at boot clears the USB network mode setting; choosing “Device” cancels host mode at boot.",
      switch: "Switch",
      confirmHost: {
        title: "Switch to host mode?",
        description:
          "USB networking and adb will disconnect: if you are using the console over USB, you will lose access, so use Wi-Fi instead. The F50 cannot power USB devices; use a powered hub.",
      },
      confirmDevice: {
        title: "Switch to device mode?",
        description:
          "USB adapters are removed from the LAN and the port goes back to USB networking with a computer.",
      },
    },
    net: {
      title: "USB networking",
      description:
        "The adapter type offered to a computer in device mode; takes effect after a reboot.",
      saved: "Saved; takes effect at next boot",
      hostRole: {
        title: "Unavailable in host mode",
        description:
          "Switch to device mode and turn off host mode at boot first.",
      },
      mode: "Network mode",
      scope: "Applies",
      permanent: "Always",
      once: "Next boot only",
      onceHint: "“Next boot only” goes back to the default (NCM) after one use",
      name: "Interface name on computer (Linux)",
    },
    adapters: {
      title: "USB adapters",
      description:
        "USB Ethernet adapters plugged in while in host mode; they can be bridged to the LAN.",
      added: "{name} added to the LAN",
      name: "Interface",
      carrier: "Cable",
      lan: "LAN",
      inLan: "In LAN",
      notInLan: "Not in LAN",
      ownerUnknown: "Owner unknown",
      owner: "Belongs to {owner}",
      add: "Add to LAN",
      adding: "Adding",
      empty: "No USB adapters found (check that the hub is powered)",
      hostOnly: {
        title: "Host mode only",
        description:
          "Switch the port to host mode to list the plugged-in USB adapters here.",
      },
    },
    adb: {
      title: "ADB debugging",
      description:
        "adb over USB (root shell). Changing it briefly interrupts the USB connection.",
      saved: "Saved",
      savedRestart:
        "Saved; adb is restarting (the USB connection drops briefly)",
      openAlert: {
        title: "adb is open",
        description:
          "Any computer plugged into the USB port gets a root shell without a password.",
      },
      damaged: {
        title: "adb setting file is damaged",
        description:
          "/etc/mu300/adb is neither on nor off, so adb stays off. Turning it off and on again fixes it.",
      },
      noKeys: {
        title: "No keys authorized yet",
        description:
          "In authorized-keys mode with no keys, no computer can connect.",
      },
      hostRole: {
        title: "Host mode",
        description:
          "adb is unavailable while the port is in host mode; it comes back in device mode.",
      },
      keys: "Authorized keys",
      keyCount: "{count} keys",
      keyCount_one: "{count} key",
      inGadget: "In USB functions",
      daemon: "Daemon",
      authReason: "Authorization note",
      enable: "Enable adb",
      access: "Access control",
      authKeys: "Authorized keys only",
      authOpen: "Open",
      savingHint: "Saving…",
      accessHint:
        "“Authorized keys only” is recommended; add your computer's adbkey.pub below",
      confirmOpen: {
        title: "Switch to open mode?",
        confirm: "Make open",
        description:
          "Once open, any computer plugged into the USB port gets a root shell on the device without a password. Use it only briefly, in a trusted place.",
      },
    },
    adbKey: {
      required: "Paste a public key",
      tooLong: "At most 800 characters",
      oneLine: "Must be a single line",
      invalid:
        "Not a valid adb public key (the contents of ~/.android/adbkey.pub, starting with QAAAA)",
      added: "Authorized; {count} keys in total",
      added_one: "Authorized; {count} key in total",
      exists: "This key is already authorized",
      label: "Authorize a computer's public key",
      hint: "Paste the contents of ~/.android/adbkey.pub from your computer",
      add: "Add key",
      adding: "Adding",
    },
  },
};
export default m;
