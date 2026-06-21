// Command litd-hub serves a static-friendly index of world archives and the
// archives themselves, with no authentication (#175, D-2026-06-11-23). It scans
// -data for *.litdworld, verifies each through the worldarchive read path, and
// exposes GET /index.json and GET /worlds/<hash>.litdworld. The index is plain
// JSON a dumb file server or CDN could serve; this binary just generates + serves
// it live.
package main

import (
	"flag"
	"log"
	"net/http"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/hub"
)

func main() {
	data := flag.String("data", "./worlds", "directory of .litdworld archives to index and serve")
	addr := flag.String("addr", ":8080", "listen address")
	engine := flag.String("engine", "", "engine version for compat filtering (empty: index all well-formed archives)")
	blocklist := flag.String("blocklist", "", "takedown blocklist file (content hashes to delist + 410); reloaded each reindex (#181)")
	flag.Parse()

	srv := hub.NewServer(*data, *engine)
	srv.SetBlocklistPath(*blocklist)
	if err := srv.Reindex(); err != nil {
		log.Fatalf("litd-hub: initial index of %q failed: %v", *data, err)
	}
	log.Printf("litd-hub: serving %q on %s (GET /index.json, GET /worlds/<hash>.litdworld)", *data, *addr)
	if err := http.ListenAndServe(*addr, srv); err != nil {
		log.Fatalf("litd-hub: %v", err)
	}
}
