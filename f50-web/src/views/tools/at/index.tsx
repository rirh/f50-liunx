import { AtTerminal } from "./components/at-terminal";

/** The guarded AT terminal over `f50.at`, filling the content area like the system log. */
export default function AtTerminalPage() {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <AtTerminal />
    </div>
  );
}
