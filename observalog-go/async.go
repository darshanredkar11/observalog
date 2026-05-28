package log

import (
	"io"
	"sync"
	"sync/atomic"
)

var (
	logCh     chan []byte
	dropCount atomic.Uint64
	wg        sync.WaitGroup
	once      sync.Once
)

// StartDrainGoroutine launches a goroutine that drains logCh to w.
// Called once at Init(). Panics if called twice.
func StartDrainGoroutine(w io.Writer, bufSize int) {
	once.Do(func() {
		logCh = make(chan []byte, bufSize)
		wg.Add(1)
		go func() {
			defer wg.Done()
			for line := range logCh {
				w.Write(line)
			}
		}()
	})
}

// SendToChannel attempts to send a log line to the buffer.
// Non-blocking: if channel full, increments drop counter and returns.
func SendToChannel(line []byte) {
	select {
	case logCh <- line:
	default:
		dropCount.Add(1)
	}
}

// FlushAndClose drains the channel and closes it.
// Blocks until all buffered logs are written.
func FlushAndClose() {
	close(logCh)
	wg.Wait()
}

// DroppedLogCount returns the number of logs dropped due to full buffer.
func DroppedLogCount() uint64 {
	return dropCount.Load()
}
