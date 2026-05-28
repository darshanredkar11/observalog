export type F = Record<string, unknown>;

export class Err {
    constructor(
        public readonly kind: string,
        public readonly code: string,
        public readonly message: string,
        public readonly retryable: boolean,
    ) {}
}

export enum Outcome {
    Success = 'success',
    Failure = 'failure',
    Partial = 'partial',
    Pending = 'pending',
}

export enum Level {
    Debug = 0,
    Info  = 1,
    Warn  = 2,
    Error = 3,
}
