import type { Messages } from "../zh";

const m: Messages["sms"] = {
  page: {
    title: "SMS",
    description:
      "Send and receive text messages; the list refreshes every 15 seconds.",
    pickTitle: "Pick a conversation",
    pickDescription:
      "Pick a conversation on the left to read it, or click “New message”.",
  },
  transport: {
    title: "The network cannot carry SMS right now",
    description:
      "The SIM has no SMS path on the network: IMS {ims}, CS domain {cs}, 5G SMS over NAS {nas}. The modem's SMS settings are fine, yet nothing can be received or sent. IMS (VoLTE/VoNR) registration, or a switch to 4G/NSA mode, is usually needed.",
    up: "up",
    down: "down",
    unknown: "unknown",
  },
  toast: {
    syncStarted: "Syncing messages from the SIM card",
    synced: "SMS sync completed",
    syncFailed: "SMS sync failed: {reason}",
    syncUnavailable:
      "The device cannot report sync results. Update the device and try again.",
    deleted: "Deleted {count} messages",
    deleted_one: "Deleted {count} message",
    deletedSim: "Deleted {count} messages ({sim} on the SIM card)",
    deletedSim_one: "Deleted {count} message ({sim} on the SIM card)",
    sent: "Sent",
    sentParts: "Sent ({count} parts)",
    sendFailed: "Could not send",
    copyFailed: "Could not copy",
  },
  form: {
    numberInvalid: "Digits only, with an optional leading +, up to 20",
    textRequired: "Enter a message",
    textTooLong: "Too long: {chars}/{max} characters",
    tooManyParts: "Needs {parts} parts; at most {max}",
  },
  actions: {
    unread: "{unread} unread / {total} total",
    storageTitle: "{store} storage, read {time}",
    storage: "SIM {used}/{total}",
    sync: "Sync",
    deleteAll: "Delete all",
    deleteAllTitle: "Delete all messages",
    deleteAllDescription:
      "Every message in the device's message store will be deleted. This cannot be undone.",
    new: "New message",
    push: "Push",
  },
  list: {
    searchLabel: "Search conversations",
    searchPlaceholder: "Search numbers or text",
    clearSearch: "Clear search",
    all: "All",
    unread: "Unread",
    count: "{count} conversations",
    count_one: "{count} conversation",
    emptyTitle: "No messages",
    emptyDescription: "Messages you receive and send show up here.",
    noMatchTitle: "No matching conversations",
    noMatchDescription: "Try other words, or switch to “All”.",
    loadMore: "Load more conversations",
    me: "Me: ",
    blank: "(empty)",
  },
  thread: {
    back: "Back to conversations",
    loading: "Loading…",
    count: "{count} messages",
    count_one: "{count} message",
    deleteConversation: "Delete conversation",
    deleteConversationTitle: "Delete the whole conversation",
    deleteConversationDescription:
      "All {count} messages with {peer} will be deleted one by one.",
    deleteConversationDescription_one:
      "The {count} message with {peer} will be deleted.",
    emptyTitle: "No messages",
    emptyDescription: "This conversation has no messages yet.",
    older: "Older messages",
    deleteOneTitle: "Delete this message",
    quote: "“{text}”",
  },
  bubble: {
    coding8bit: "8-bit",
    parts: "{count} parts",
    incomplete: "Incomplete {have}/{total}",
    incompleteTitle: "Some parts of this long message have not arrived yet",
    menu: "Message actions",
    copyText: "Copy text",
  },
  composer: {
    notNumber: "This sender is not a phone number; you cannot reply.",
    jobTitle: "Sending SMS",
    textLabel: "Message",
    placeholder: "Type a message…",
    send: "Send",
    sending: "Sending",
  },
  newMessage: {
    title: "New message",
    description:
      "Up to 10 parts: GSM-7 holds 160 characters per message (153 per part), text with Chinese or other characters 70 (67).",
    number: "Number",
    text: "Message",
  },
  push: {
    loadFailed: "Could not read the push settings",
    title: "Telegram push",
    description:
      "Every received SMS is forwarded to a chat by your Telegram bot.",
    enabled: "Push received SMS",
    enabledDescription:
      "When off, nothing is pushed and anything still queued is dropped.",
    token: "Bot token",
    tokenHint:
      "Send /newbot to @BotFather in Telegram to get one. The device keeps it and never shows it again.",
    tokenKeep: "Set ({hint}); leave empty to keep it",
    tokenInvalid: "Expected digits:letters, e.g. 123456789:AAH…",
    tokenRequired: "Enter the bot token",
    chat: "Chat ID",
    chatHint:
      "A user's or group's numeric ID (groups start with -100), or a public channel's @username. Message the bot first (or add it to the group or channel), or it cannot write to you.",
    chatInvalid: "A numeric ID (may start with -) or an @channel",
    chatRequired: "Enter the chat ID",
    chatIsBot:
      "This is the bot's own ID (the digits before the token's colon). Enter your own or a group's chat ID: message the bot first, then read it from getUpdates or @userinfobot.",
    api: "API base",
    apiHint:
      "Leave empty for the official API. If Telegram cannot be reached directly, enter a self-hosted Bot API or a reverse proxy.",
    apiInvalid: "An http(s):// address without a query",
    proxy: "Send through the proxy",
    proxyDescription:
      "Reaches Telegram through the proxy while it runs; when off, always connects directly.",
    proxyIdle:
      "The proxy is not running, so this connects directly; once the proxy runs, pushes go through it.",
    test: "Send test",
    testSent: "Test message sent; check Telegram",
    saved: "Push settings saved",
    lastOk: "Last push succeeded · {ago}",
    lastFailed: "Last push failed · {ago}",
    queued: "{count} queued",
    queued_one: "{count} queued",
  },
  deleteDialog: {
    alsoSim: "Also delete the copies on the SIM card",
    alsoSimDescription:
      "Unchecked, messages are deleted only from the device's store; deleted messages are not synced back from the SIM card.",
  },
  counter: {
    titleGsm: "{chars} characters · {units} septets · {perPart} per part",
    titleUcs2: "{chars} characters · {units} UTF-16 units · {perPart} per part",
    text: "{coding} · {parts}/{max} parts · {remaining} left",
  },
};
export default m;
