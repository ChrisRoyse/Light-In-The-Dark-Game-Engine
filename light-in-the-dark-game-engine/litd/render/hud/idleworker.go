package hud

// IdleWorkerButton (#197): the idle-worker cycle button above the minimap.
// Visible only when ≥1 worker is idle; each click advances to the next idle
// worker (round-robin) so repeat clicks tour all of them. Pure state machine:
// the caller feeds the current idle count each refresh and calls Click() on a
// press. Command emission (select the worker + center the camera on it) is the
// caller's job via the input pipeline — this widget owns visibility and which
// worker is next, identified by its index into the caller's idle list.
type IdleWorkerState struct {
	IdleCount int // idle workers this refresh
}

type IdleWorkerUpdate struct {
	Dirty   bool `json:"dirty"`
	Visible bool `json:"visible"` // shown only when IdleCount > 0
	Count   int  `json:"count"`
}

// IdleWorkerClick names which idle worker (index into the caller's idle list)
// to select + center. HasTarget is false when none are idle (button hidden).
type IdleWorkerClick struct {
	HasTarget bool `json:"hasTarget"`
	Index     int  `json:"index"`
}

type IdleWorkerButton struct {
	cursor      int
	count       int
	initialized bool
}

// Update folds the latest idle count. Visible iff > 0; dirty on any change
// (appear/disappear/count). If the list shrank past the cursor, the cycle
// restarts at 0 so a click never points off the end.
func (w *IdleWorkerButton) Update(s IdleWorkerState) IdleWorkerUpdate {
	c := s.IdleCount
	if c < 0 {
		c = 0
	}
	dirty := !w.initialized || c != w.count
	if w.cursor >= c {
		w.cursor = 0
	}
	w.count = c
	w.initialized = true
	return IdleWorkerUpdate{Dirty: dirty, Visible: c > 0, Count: c}
}

// Click advances the round-robin cursor and returns the idle worker to act on.
// No-op (HasTarget false) when none are idle.
func (w *IdleWorkerButton) Click() IdleWorkerClick {
	if w.count <= 0 {
		return IdleWorkerClick{}
	}
	idx := w.cursor
	w.cursor = (w.cursor + 1) % w.count
	return IdleWorkerClick{HasTarget: true, Index: idx}
}
