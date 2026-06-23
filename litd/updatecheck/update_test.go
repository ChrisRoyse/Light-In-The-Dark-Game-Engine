package updatecheck

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// #186 FSV: the client update check decides notify / up-to-date / skip against
// synthetic manifests served over a REAL httptest server through the REAL
// HTTPFetcher (no mock network). SoT = the returned Result (outcome + artifact +
// reason) and the parsed manifest. Edges: equal version → no notify; offline →
// silent skip; malformed JSON → skip, no crash; plus older-version, dev-build,
// missing-OS-artifact, and non-200.

func serve(t *testing.T, status int, body string) string {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(status)
		io.WriteString(w, body)
	}))
	t.Cleanup(srv.Close)
	return srv.URL
}

func manifestJSON(t *testing.T, version string, arts ...Artifact) string {
	t.Helper()
	b, err := json.Marshal(Manifest{LatestVersion: version, Artifacts: arts})
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func TestUpdateCheckFSV(t *testing.T) {
	lin := Artifact{OS: "linux", URL: "https://litd.example/dl/linux", SHA256: "aa11"}
	win := Artifact{OS: "windows", URL: "https://litd.example/dl/win", SHA256: "bb22"}

	// happy path: a v0.1.0 build vs a manifest advertising v0.2.0 → notify, with
	// the linux artifact's URL + sha for the menu link.
	t.Run("NewerAdvertised_Notify", func(t *testing.T) {
		body := manifestJSON(t, "v0.2.0", lin, win)
		t.Logf("BEFORE: running=v0.1.0 osKey=linux manifest=%s", body)
		got := Check("v0.1.0", "linux", HTTPFetcher{URL: serve(t, 200, body)})
		t.Logf("AFTER: outcome=%s latest=%s artifact=%+v reason=%q", got.Outcome, got.Latest, got.Artifact, got.Reason)
		if got.Outcome != OutcomeUpdateAvailable {
			t.Fatalf("outcome=%s, want update-available (reason=%s)", got.Outcome, got.Reason)
		}
		if got.Artifact == nil || got.Artifact.URL != lin.URL || got.Artifact.SHA256 != "aa11" {
			t.Fatalf("artifact=%+v, want linux %s/aa11", got.Artifact, lin.URL)
		}
	})

	// edge 1: manifest version equals the running version → no notification.
	t.Run("EqualVersion_NoNotify", func(t *testing.T) {
		got := Check("v0.2.0", "linux", HTTPFetcher{URL: serve(t, 200, manifestJSON(t, "v0.2.0", lin))})
		t.Logf("AFTER: running=v0.2.0 latest=v0.2.0 → outcome=%s reason=%q", got.Outcome, got.Reason)
		if got.Outcome != OutcomeUpToDate || got.Artifact != nil {
			t.Fatalf("equal version: outcome=%s artifact=%+v, want up-to-date / nil", got.Outcome, got.Artifact)
		}
	})

	// older advertised (downgrade manifest) → still up-to-date, no notify.
	t.Run("OlderAdvertised_NoNotify", func(t *testing.T) {
		got := Check("v0.2.0", "linux", HTTPFetcher{URL: serve(t, 200, manifestJSON(t, "v0.1.0", lin))})
		t.Logf("AFTER: running=v0.2.0 latest=v0.1.0 → outcome=%s", got.Outcome)
		if got.Outcome != OutcomeUpToDate {
			t.Fatalf("older advertised: outcome=%s, want up-to-date", got.Outcome)
		}
	})

	// edge 2: site unreachable → silent skip (real refused connection).
	t.Run("Offline_SilentSkip", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
		url := srv.URL
		srv.Close() // now refused
		got := Check("v0.1.0", "linux", HTTPFetcher{URL: url, Client: &http.Client{Timeout: 2 * time.Second}})
		t.Logf("AFTER: offline → outcome=%s reason=%q", got.Outcome, got.Reason)
		if got.Outcome != OutcomeSkipped || !strings.Contains(got.Reason, "fetch:") {
			t.Fatalf("offline: outcome=%s reason=%q, want skipped/fetch", got.Outcome, got.Reason)
		}
	})

	// edge 3: malformed manifest JSON → skip, logged reason, no crash.
	t.Run("MalformedJSON_Skip", func(t *testing.T) {
		got := Check("v0.1.0", "linux", HTTPFetcher{URL: serve(t, 200, "{ not valid json")})
		t.Logf("AFTER: malformed → outcome=%s reason=%q", got.Outcome, got.Reason)
		if got.Outcome != OutcomeSkipped || !strings.Contains(got.Reason, "decode") {
			t.Fatalf("malformed: outcome=%s reason=%q, want skipped/decode", got.Outcome, got.Reason)
		}
	})

	// dev / unstamped build cannot be compared → skip without even fetching.
	t.Run("DevBuild_Skip", func(t *testing.T) {
		hit := false
		srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { hit = true }))
		t.Cleanup(srv.Close)
		got := Check("dev", "linux", HTTPFetcher{URL: srv.URL})
		t.Logf("AFTER: running=dev → outcome=%s reason=%q (server hit=%v)", got.Outcome, got.Reason, hit)
		if got.Outcome != OutcomeSkipped {
			t.Fatalf("dev build: outcome=%s, want skipped", got.Outcome)
		}
		if hit {
			t.Fatal("dev build should skip BEFORE fetching the manifest")
		}
	})

	// update available but no artifact advertised for the running OS → notify the
	// version, no direct download link.
	t.Run("UpdateButNoArtifactForOS", func(t *testing.T) {
		got := Check("v0.1.0", "linux", HTTPFetcher{URL: serve(t, 200, manifestJSON(t, "v0.2.0", win))})
		t.Logf("AFTER: v0.2.0 advertised, only windows artifact, osKey=linux → outcome=%s artifact=%+v", got.Outcome, got.Artifact)
		if got.Outcome != OutcomeUpdateAvailable || got.Artifact != nil {
			t.Fatalf("no-os-artifact: outcome=%s artifact=%+v, want update-available / nil", got.Outcome, got.Artifact)
		}
	})

	// non-200 response → skip.
	t.Run("Non200_Skip", func(t *testing.T) {
		got := Check("v0.1.0", "linux", HTTPFetcher{URL: serve(t, 404, "nope")})
		t.Logf("AFTER: http 404 → outcome=%s reason=%q", got.Outcome, got.Reason)
		if got.Outcome != OutcomeSkipped || !strings.Contains(got.Reason, "404") {
			t.Fatalf("non-200: outcome=%s reason=%q, want skipped/404", got.Outcome, got.Reason)
		}
	})
}

// ParseManifest fails closed on malformed/invalid manifests — inspected directly.
func TestParseManifestFailsClosedFSV(t *testing.T) {
	bad := []struct{ name, body string }{
		{"not json", "{nope"},
		{"empty version", `{"latestVersion":"","artifacts":[]}`},
		{"bad version", `{"latestVersion":"banana","artifacts":[]}`},
		{"unknown field", `{"latestVersion":"v0.2.0","extra":true}`},
		{"artifact missing url", `{"latestVersion":"v0.2.0","artifacts":[{"os":"linux"}]}`},
	}
	for _, c := range bad {
		if _, err := ParseManifest([]byte(c.body)); err == nil {
			t.Errorf("%s: ParseManifest accepted %q (want error)", c.name, c.body)
		} else {
			t.Logf("FSV reject %s → %v", c.name, err)
		}
	}
	// a well-formed manifest parses and round-trips its fields.
	m, err := ParseManifest([]byte(`{"latestVersion":"v0.3.1","artifacts":[{"os":"macos","url":"https://x/y","sha256":"cc33"}]}`))
	if err != nil {
		t.Fatalf("valid manifest rejected: %v", err)
	}
	if m.LatestVersion != "v0.3.1" || len(m.Artifacts) != 1 || m.Artifacts[0].SHA256 != "cc33" {
		t.Fatalf("parsed manifest = %+v, want v0.3.1 + 1 macos artifact", m)
	}
	t.Logf("FSV accept: %+v", m)
}
