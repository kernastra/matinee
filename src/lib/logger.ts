type LogArguments = unknown[];

export function debugInfo(...arguments_: LogArguments) {
  if (import.meta.env.DEV) console.info(...arguments_);
}

export function debugWarn(...arguments_: LogArguments) {
  if (import.meta.env.DEV) console.warn(...arguments_);
}

export function debugError(...arguments_: LogArguments) {
  if (import.meta.env.DEV) console.error(...arguments_);
}
