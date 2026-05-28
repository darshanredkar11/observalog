import { Level } from './fields';
import { setMinLevel, setServiceCode, resolveServiceCode } from './emit';
import { configure as configureDrain, flush } from './async-drain';
import { validateNoDictCollisions } from './dict';

export interface Config {
    serviceName: string;
    version:     string;
    env:         string;
    level?:      string; // "debug" | "info" | "warn" | "error"
    bufferSize?: number;
}

export function configFromEnv(version: string): Config {
    return {
        serviceName: process.env.SERVICE_NAME ?? '',
        version,
        env:        process.env.ENV ?? '',
        level:      (process.env.LOG_LEVEL ?? 'info').toLowerCase(),
        bufferSize: process.env.LOG_BUFFER_SIZE ? parseInt(process.env.LOG_BUFFER_SIZE, 10) : 10_000,
    };
}

export function init(cfg: Config): void {
    if (!cfg.serviceName) throw new Error('SERVICE_NAME required');
    if (!cfg.env)         throw new Error('ENV required');

    const level = cfg.level ?? 'info';
    const levelMap: Record<string, Level> = {
        debug: Level.Debug, info: Level.Info, warn: Level.Warn, error: Level.Error,
    };
    const resolvedLevel = levelMap[level];
    if (resolvedLevel === undefined) throw new Error(`Invalid LOG_LEVEL=${level}`);

    setMinLevel(resolvedLevel);
    setServiceCode(resolveServiceCode(cfg.serviceName));
    configureDrain(cfg.bufferSize ?? 10_000);
    validateNoDictCollisions();
}

export async function shutdown(): Promise<void> {
    await flush();
}
