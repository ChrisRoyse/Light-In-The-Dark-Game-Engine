// Command changelog generates a changelog draft from conventional commits in a
// git range (#184, docs/release/versioning.md). It is the generated-draft step
// of the release process — the output is hand-edited before publish, never the
// published artifact. The grouping logic lives in changelog.go (Generate),
// kept pure so it is unit-tested without git.
//
//	go run ./tools/changelog -from v0.1.0 -to HEAD -version v0.2.0
//
// With no -from, the previous tag reachable from -to is used; with no -version,
// `git describe --tags` of -to is used.
package main

import (
	"flag"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

func main() {
	from := flag.String("from", "", "start ref (exclusive); default = previous tag before -to")
	to := flag.String("to", "HEAD", "end ref (inclusive)")
	version := flag.String("version", "", "version heading; default = git describe --tags of -to")
	flag.Parse()

	if *from == "" {
		prev, err := git("describe", "--tags", "--abbrev=0", *to+"^")
		if err == nil {
			*from = strings.TrimSpace(prev)
		}
	}
	if *version == "" {
		if d, err := git("describe", "--tags", *to); err == nil {
			*version = strings.TrimSpace(d)
		} else {
			*version = "Unreleased"
		}
	}

	rng := *to
	if *from != "" {
		rng = *from + ".." + *to
	}
	out, err := git("log", rng, "--no-merges", "--pretty=%s")
	if err != nil {
		fmt.Fprintf(os.Stderr, "changelog: git log %s failed: %v\n", rng, err)
		os.Exit(1)
	}
	subjects := strings.Split(strings.TrimRight(out, "\n"), "\n")
	fmt.Print(Generate(*version, subjects))
}

func git(args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	var out strings.Builder
	cmd.Stdout = &out
	if err := cmd.Run(); err != nil {
		return "", err
	}
	return out.String(), nil
}
