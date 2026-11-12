/** `f50.cpu get` / `profile` (f50-system/src/cpu.rs, thermal.rs). */

export type CpuProfile = "saving" | "balanced" | "performance";

/** One cpufreq policy (a cluster); frequencies in MHz. */
export interface CpuPolicy {
  name: string;
  /** "0-3": the first and last related CPU. */
  cpus: string;
  governor: string | null;
  cur: number | null;
  min: number | null;
  max: number | null;
  hwmin: number | null;
  hwmax: number | null;
}

export interface ThermalTrip {
  /** passive, active, hot, critical. */
  type: string;
  /** m°C. */
  temp: number | null;
}

export interface CoolingState {
  type: string;
  state: number;
  max: number;
}

/** thermal::ZoneDetail. */
export interface ThermalZone {
  zone: string;
  /** m°C. */
  temp: number | null;
  /** enabled / disabled. */
  mode: string | null;
  trips: ThermalTrip[];
  cooling: CoolingState[];
}

export interface CpuStatus {
  profile: CpuProfile;
  /** A profile was chosen and saved (else the default applies). */
  saved: boolean;
  profiles: CpuProfile[];
  /** A cpufreq cooling device is capping, or the thermal guard's alarm is on. */
  throttling: boolean;
  thermal: ThermalZone[];
  policies: CpuPolicy[];
}
