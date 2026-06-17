package litd

// Campaign storage (#242; hashtable-and-gamecache.md, public-api-design.md
// §2 row 1: gamecache → Game.Storage()). The JASS gamecache stored typed
// values under a (missionKey, key) pair and synced/persisted them for
// cross-map campaign state. Canonical Go is one Storage reached from the
// Game: typed Set/Get with comma-ok, plus an explicit versioned Save/Load
// to any io stream (the file under the user profile is the caller's
// choice — the API stays I/O-agnostic and testable).
//
// SyncStored* is intentionally absent (deferred to the M7 lockstep work):
// storage is local until networking lands.

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"sort"
)

// storageMagic + storageVersion guard the persisted format.
const (
	storageMagic   = "LITDSTOR"
	storageVersion = uint8(1)
)

type storageKey struct{ category, key string }

// Storage is the typed campaign key-value store. Values live in
// per-type maps keyed by (category, key); iteration is only ever the
// sorted serialization path, never gameplay logic, so the maps are safe.
type Storage struct {
	ints  map[storageKey]int64
	reals map[storageKey]float64
	strs  map[storageKey]string
	bools map[storageKey]bool
}

// Storage returns the game's campaign store, creating it on first use.
// Nil-safe (returns nil on a nil game). JASS: InitGameCache / the
// bj_lastCreatedGameCache singleton.
// JASS: InitGameCache
func (g *Game) Storage() *Storage {
	if g == nil {
		return nil
	}
	if g.storage == nil {
		g.storage = newStorage()
	}
	return g.storage
}

func newStorage() *Storage {
	return &Storage{
		ints:  map[storageKey]int64{},
		reals: map[storageKey]float64{},
		strs:  map[storageKey]string{},
		bools: map[storageKey]bool{},
	}
}

// SetInt / GetInt store and read an integer. JASS: StoreInteger /
// GetStoredInteger + HaveStoredInteger (comma-ok).
// JASS: StoreInteger
func (s *Storage) SetInt(category, key string, v int) {
	if s != nil {
		s.ints[storageKey{category, key}] = int64(v)
	}
}

// GetInt reads a stored integer; ok is false if absent (see SetInt). JASS:
// GetStoredInteger + HaveStoredInteger.
// JASS: GetStoredInteger
func (s *Storage) GetInt(category, key string) (int, bool) {
	if s == nil {
		return 0, false
	}
	v, ok := s.ints[storageKey{category, key}]
	return int(v), ok
}

// SetReal / GetReal store and read a float. JASS: StoreReal.
func (s *Storage) SetReal(category, key string, v float64) {
	if s != nil {
		s.reals[storageKey{category, key}] = v
	}
}

// GetReal reads a stored float; ok is false if absent (see SetReal). JASS:
// GetStoredReal + HaveStoredReal.
func (s *Storage) GetReal(category, key string) (float64, bool) {
	if s == nil {
		return 0, false
	}
	v, ok := s.reals[storageKey{category, key}]
	return v, ok
}

// SetString / GetString store and read a string. JASS: StoreString.
func (s *Storage) SetString(category, key, v string) {
	if s != nil {
		s.strs[storageKey{category, key}] = v
	}
}

// GetString reads a stored string; ok is false if absent (see SetString).
// JASS: GetStoredString + HaveStoredString.
func (s *Storage) GetString(category, key string) (string, bool) {
	if s == nil {
		return "", false
	}
	v, ok := s.strs[storageKey{category, key}]
	return v, ok
}

// SetBool / GetBool store and read a boolean. JASS: StoreBoolean.
func (s *Storage) SetBool(category, key string, v bool) {
	if s != nil {
		s.bools[storageKey{category, key}] = v
	}
}

// GetBool reads a stored boolean; ok is false if absent (see SetBool). JASS:
// GetStoredBoolean + HaveStoredBoolean.
func (s *Storage) GetBool(category, key string) (bool, bool) {
	if s == nil {
		return false, false
	}
	v, ok := s.bools[storageKey{category, key}]
	return v, ok
}

// Clear empties the whole store. JASS: FlushGameCache.
// JASS: FlushGameCache
func (s *Storage) Clear() {
	if s == nil {
		return
	}
	*s = *newStorage()
}

func sortedKeys[V any](m map[storageKey]V) []storageKey {
	ks := make([]storageKey, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Slice(ks, func(i, j int) bool {
		if ks[i].category != ks[j].category {
			return ks[i].category < ks[j].category
		}
		return ks[i].key < ks[j].key
	})
	return ks
}

// Save writes the store to w in a versioned, deterministic (sorted-key)
// binary form — safe to persist under a user profile for cross-map
// campaign state. JASS: SaveGameCache.
func (s *Storage) Save(w io.Writer) error {
	if s == nil {
		return fmt.Errorf("litd: Storage.Save on nil store")
	}
	bw := bufio.NewWriter(w)
	if _, err := bw.WriteString(storageMagic); err != nil {
		return err
	}
	if err := bw.WriteByte(storageVersion); err != nil {
		return err
	}
	wU32 := func(v uint32) error {
		var b [4]byte
		binary.LittleEndian.PutUint32(b[:], v)
		_, e := bw.Write(b[:])
		return e
	}
	wU64 := func(v uint64) error {
		var b [8]byte
		binary.LittleEndian.PutUint64(b[:], v)
		_, e := bw.Write(b[:])
		return e
	}
	wStr := func(str string) error {
		if err := wU32(uint32(len(str))); err != nil {
			return err
		}
		_, e := bw.WriteString(str)
		return e
	}
	wKey := func(k storageKey) error {
		if err := wStr(k.category); err != nil {
			return err
		}
		return wStr(k.key)
	}
	// section order is fixed; within a section keys are sorted.
	if err := wU32(uint32(len(s.ints))); err != nil {
		return err
	}
	for _, k := range sortedKeys(s.ints) {
		if err := wKey(k); err != nil {
			return err
		}
		if err := wU64(uint64(s.ints[k])); err != nil {
			return err
		}
	}
	if err := wU32(uint32(len(s.reals))); err != nil {
		return err
	}
	for _, k := range sortedKeys(s.reals) {
		if err := wKey(k); err != nil {
			return err
		}
		if err := wU64(math.Float64bits(s.reals[k])); err != nil {
			return err
		}
	}
	if err := wU32(uint32(len(s.strs))); err != nil {
		return err
	}
	for _, k := range sortedKeys(s.strs) {
		if err := wKey(k); err != nil {
			return err
		}
		if err := wStr(s.strs[k]); err != nil {
			return err
		}
	}
	if err := wU32(uint32(len(s.bools))); err != nil {
		return err
	}
	for _, k := range sortedKeys(s.bools) {
		if err := wKey(k); err != nil {
			return err
		}
		b := byte(0)
		if s.bools[k] {
			b = 1
		}
		if err := bw.WriteByte(b); err != nil {
			return err
		}
	}
	return bw.Flush()
}

// Load replaces the store's contents with the stream written by Save.
// Fails closed on a bad magic or version mismatch (no partial load).
// JASS: ReloadGameCachesFromDisk.
func (s *Storage) Load(r io.Reader) error {
	if s == nil {
		return fmt.Errorf("litd: Storage.Load on nil store")
	}
	br := bufio.NewReader(r)
	magic := make([]byte, len(storageMagic))
	if _, err := io.ReadFull(br, magic); err != nil {
		return err
	}
	if string(magic) != storageMagic {
		return fmt.Errorf("litd: Storage.Load: bad magic %q", magic)
	}
	ver, err := br.ReadByte()
	if err != nil {
		return err
	}
	if ver != storageVersion {
		return fmt.Errorf("litd: Storage.Load: version %d, this build reads %d", ver, storageVersion)
	}
	rU32 := func() (uint32, error) {
		var b [4]byte
		_, e := io.ReadFull(br, b[:])
		return binary.LittleEndian.Uint32(b[:]), e
	}
	rU64 := func() (uint64, error) {
		var b [8]byte
		_, e := io.ReadFull(br, b[:])
		return binary.LittleEndian.Uint64(b[:]), e
	}
	rStr := func() (string, error) {
		n, e := rU32()
		if e != nil {
			return "", e
		}
		buf := make([]byte, n)
		if _, e := io.ReadFull(br, buf); e != nil {
			return "", e
		}
		return string(buf), nil
	}
	rKey := func() (storageKey, error) {
		c, e := rStr()
		if e != nil {
			return storageKey{}, e
		}
		k, e := rStr()
		return storageKey{c, k}, e
	}
	next := newStorage()
	// ints
	n, err := rU32()
	if err != nil {
		return err
	}
	for i := uint32(0); i < n; i++ {
		k, e := rKey()
		if e != nil {
			return e
		}
		v, e := rU64()
		if e != nil {
			return e
		}
		next.ints[k] = int64(v)
	}
	// reals
	if n, err = rU32(); err != nil {
		return err
	}
	for i := uint32(0); i < n; i++ {
		k, e := rKey()
		if e != nil {
			return e
		}
		v, e := rU64()
		if e != nil {
			return e
		}
		next.reals[k] = math.Float64frombits(v)
	}
	// strings
	if n, err = rU32(); err != nil {
		return err
	}
	for i := uint32(0); i < n; i++ {
		k, e := rKey()
		if e != nil {
			return e
		}
		v, e := rStr()
		if e != nil {
			return e
		}
		next.strs[k] = v
	}
	// bools
	if n, err = rU32(); err != nil {
		return err
	}
	for i := uint32(0); i < n; i++ {
		k, e := rKey()
		if e != nil {
			return e
		}
		b, e := br.ReadByte()
		if e != nil {
			return e
		}
		next.bools[k] = b != 0
	}
	*s = *next
	return nil
}
