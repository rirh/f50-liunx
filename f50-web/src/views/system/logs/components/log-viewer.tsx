import { cn } from "@/lib/utils";

/** "Fri Nov 13 18:48:16 2026 kern.warn kernel: …": the time, the facility.level, then the message. */
const SYSLOG = /^(\w{3} \w{3} +\d+ [\d:]{8} \d{4}) (\S+)\.(\w+) (.*)$/;

const LEVEL_TONE: Record<string, string> = {
  emerg: "text-[#ff7b72]",
  alert: "text-[#ff7b72]",
  crit: "text-[#ff7b72]",
  err: "text-[#ff7b72]",
  warn: "text-[#d29922]",
  notice: "text-[#79c0ff]",
};

function Line({ text }: { text: string }) {
  if (text.startsWith("--- "))
    return (
      <span className="mt-3 block font-semibold text-[#58a6ff] first:mt-0">
        {text}
      </span>
    );
  const m = SYSLOG.exec(text);
  if (!m) return <span className="block">{text || " "}</span>;
  const [, time, facility, level, message] = m;
  const tone = LEVEL_TONE[level];
  return (
    <span className={cn("block", tone)}>
      <span className="text-[#6e7681]">{time}</span>{" "}
      <span className={tone ?? "text-[#8b949e]"}>
        {facility}.{level}
      </span>{" "}
      <span className={tone ?? "text-[#c9d1d9]"}>{message}</span>
    </span>
  );
}

/** The log lines inside a {@link Terminal}: the time dimmed, warnings amber, errors red, section headers blue. */
export function LogLines({ lines, empty }: { lines: string[]; empty: string }) {
  if (lines.length === 0)
    return <p className="font-sans text-[#8b949e]">{empty}</p>;
  return (
    <pre className="font-mono whitespace-pre-wrap break-all">
      {lines.map((l, i) => (
        <Line key={i} text={l} />
      ))}
    </pre>
  );
}
