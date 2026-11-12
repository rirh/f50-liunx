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
  toast: {
    syncStarted: "Syncing messages from the SIM card",
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
    storage: "SIM storage {used}/{total} · {ago}",
    sync: "Sync",
    syncing: "Syncing",
    deleteAll: "Delete all",
    deleting: "Deleting",
    deleteAllTitle: "Delete all messages",
    deleteAllDescription:
      "Every message in the device's message store will be deleted. This cannot be undone.",
    new: "New message",
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
    unread: "Unread",
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
