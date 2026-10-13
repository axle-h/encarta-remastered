/**
 * The command interpreter core (tours-3d.md §3.2–§3.6): macros, alarms, events, the Timer flag and
 * dispatch of every other step to the control's subsystem handlers.
 */
import type { Step, StepOf } from "../types";
import { parseCommand } from "./parse";

export interface Handlers {
  /** Runs one subsystem step (camera, walk map, lights, 3D, 2D, scene settings). */
  apply(step: Step): string;
  /** Answers a `…?` query. */
  query(step: StepOf<"query">): string;
  /** `Event3DX(text)` to the host. */
  fireEvent(text: string): void;
  /** `SCENE.DELETEALL`: reset everything the handlers own. */
  reset(): void;
}

interface AlarmState {
  name: string;
  active: boolean;
  maxExecutions: number;
  interval: number;
  macroText: string;
  steps: Step[];
  count: number;
  /** Clock start in seconds, or null when the clock is not running. */
  clock: number | null;
  sequence: string[];
}

const MAX_DEPTH = 32;

export class Interpreter {
  readonly macros = new Map<string, Step[]>();
  /** Alarms in creation order (polling order). */
  readonly alarms: AlarmState[] = [];
  alarmsEnabled = false;
  timerOn = true;
  /** Seconds; supplied by the tick so alarms follow the simulation clock. */
  now = 0;
  private depth = 0;

  constructor(private readonly handlers: Handlers) {}

  /** `Do(command)`: one command string through the interpreter; results joined with `;`. */
  do(command: string): string {
    return this.run(parseCommand(command));
  }

  run(steps: readonly Step[]): string {
    if (this.depth > MAX_DEPTH) return "Failed";
    this.depth++;
    try {
      const results: string[] = [];
      for (const step of steps) {
        const r = this.exec(step);
        if (r !== null) results.push(r);
      }
      return results.join(";");
    } finally {
      this.depth--;
    }
  }

  /** Runs one step; null = no result (a statement with no known subsystem). */
  exec(step: Step): string | null {
    switch (step.op) {
      case "noop":
        return step.reason.startsWith("no known subsystem") ? null : "Failed";
      case "defineMacro":
        this.macros.set(step.name.toUpperCase(), step.steps ?? (step.body !== undefined ? parseCommand(step.body) : []));
        return "Success";
      case "deleteMacro":
        return this.macros.delete(step.name.toUpperCase()) ? "Success" : "Failed";
      case "runMacros": {
        const out: string[] = [];
        for (const name of step.names) {
          const body = this.macros.get(name.toUpperCase());
          out.push(body ? this.run(body) : "Failed");
        }
        return out.join(";");
      }
      case "event":
        this.handlers.fireEvent(step.text);
        return "Success";
      case "timer":
        this.timerOn = step.on;
        return "Success";
      case "alarmNew":
        return this.alarmNew(step);
      case "alarmsEnable":
        this.alarmsEnabled = step.on;
        for (const a of this.alarms) a.clock = null;
        return "Success";
      case "alarm":
        return this.alarmCommand(step);
      case "deleteAll":
        this.macros.clear();
        this.alarms.length = 0;
        this.handlers.reset();
        return "Success";
      case "query":
        if (step.subsystem === "TIMER") return this.timerOn ? "1" : "0";
        return this.handlers.query(step);
      default:
        return this.handlers.apply(step);
    }
  }

  private alarmNew(step: StepOf<"alarmNew">): string {
    const name = step.name.toUpperCase();
    const existing = this.alarms.findIndex((a) => a.name === name);
    if (existing >= 0) this.alarms.splice(existing, 1);
    this.alarms.push({
      name,
      active: step.active,
      maxExecutions: step.maxExecutions,
      interval: step.interval,
      macroText: step.macroText,
      steps: step.steps,
      count: 0,
      clock: null,
      sequence: [],
    });
    return "Success";
  }

  private alarmCommand(step: StepOf<"alarm">): string {
    const i = this.alarms.findIndex((a) => a.name === step.name.toUpperCase());
    const a = this.alarms[i];
    if (!a) return "Failed";
    switch (step.action) {
      case "activate":
        a.active = true;
        a.count = 0;
        a.clock = null;
        break;
      case "deactivate":
        a.active = false;
        a.clock = null;
        break;
      case "toggle":
        a.active = !a.active;
        a.clock = null;
        break;
      case "delete":
        this.alarms.splice(i, 1);
        break;
      case "addSequence":
        a.sequence.push(step.value ?? "");
        break;
      case "clearSequence":
        a.sequence.length = 0;
        break;
      default:
        return "Failed";
    }
    return "Success";
  }

  /** Once per tick (§3.5): each alarm fires at most once; late firings are not caught up. */
  pollAlarms(now: number): void {
    this.now = now;
    if (!this.alarmsEnabled) return;
    for (const a of [...this.alarms]) {
      if (!this.alarms.includes(a)) continue;
      if (a.clock === null) {
        if (!a.active) continue;
        a.clock = now;
        a.count = 0;
      }
      if (a.active && now - a.clock >= a.interval) {
        const seq = a.sequence[a.count];
        if (a.count < a.sequence.length && seq !== undefined) this.do(a.macroText.split("@").join(seq));
        else this.run(a.steps);
        a.count++;
        if (a.maxExecutions >= 0 && a.count >= a.maxExecutions) a.active = false;
        a.clock = now;
      }
    }
  }
}
