export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

export interface LogLine {
	text: string
	level: LogLevel | null
	/** Set on the "repeated N more times" line that stands in for repeats of the entry above it. */
	repeat?: number
}

export interface Log4jEvent {
	logger_name?: string
	level?: string
	thread_name?: string
	timestamp_millis?: number
	message?: string
	throwable?: string
}

export interface LogSource {
	id: string
	name: string
	live: boolean
}
