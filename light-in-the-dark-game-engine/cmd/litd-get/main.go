// Command litd-get downloads a world from a hub and installs it into a local
// worlds directory, with content-hash + engine-version verification (#178). It
// is the operable front end to litd/hub.Client: list what a hub offers, or fetch
// one world by content hash. A download that fails any gate (hash mismatch,
// engine-version incompatibility, load-time verification) is refused loudly and
// nothing is installed.
//
//	litd-get -hub URL [-worlds DIR] [-engine X.Y.Z] -list
//	litd-get -hub URL [-worlds DIR] [-engine X.Y.Z] -get <content-hash>
package main

import (
	"context"
	"flag"
	"fmt"
	"os"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/hub"
)

func main() {
	hubURL := flag.String("hub", "", "hub base URL, e.g. http://localhost:8080")
	worldsDir := flag.String("worlds", "./worlds", "local worlds directory to install into")
	engine := flag.String("engine", "", "this build's engine version (gates playability; empty = dev, no version gate)")
	list := flag.Bool("list", false, "list the worlds the hub offers and exit")
	get := flag.String("get", "", "content hash of the world to download and install")
	flag.Parse()

	if *hubURL == "" {
		fmt.Fprintln(os.Stderr, "litd-get: -hub is required")
		os.Exit(2)
	}
	cli := hub.NewClient(*hubURL, *worldsDir, *engine, nil)
	ctx := context.Background()

	idx, err := cli.FetchIndex(ctx)
	if err != nil {
		fmt.Fprintln(os.Stderr, "litd-get:", err)
		os.Exit(1)
	}

	if *list || *get == "" {
		fmt.Printf("hub %s offers %d world(s):\n", *hubURL, len(idx.Worlds))
		for _, e := range idx.Worlds {
			fmt.Printf("  %s  %-24s engine=%s  %d bytes\n", e.Hash, e.Title, e.EngineRange, e.SizeBytes)
		}
		return
	}

	var entry hub.Entry
	found := false
	for _, e := range idx.Worlds {
		if e.Hash == *get {
			entry, found = e, true
			break
		}
	}
	if !found {
		fmt.Fprintf(os.Stderr, "litd-get: hub has no world with hash %s\n", *get)
		os.Exit(1)
	}

	dest, err := cli.Install(ctx, entry)
	if err != nil {
		fmt.Fprintln(os.Stderr, "litd-get:", err)
		os.Exit(1)
	}
	fmt.Printf("installed %q (%s) -> %s\n", entry.Title, entry.Hash, dest)
}
