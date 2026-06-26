package telemetry

import (
	"bytes"
	"fmt"
	"net/http"
	"time"
)

// Client posts telemetry to the ingest endpoint — but only when explicitly
// enabled. Off by default (R-OBS-4): a zero-value Client never touches the
// network. There is no retry: a failed send WARNs at most once per client so an
// unreachable endpoint cannot cause a retry storm or affect gameplay.
type Client struct {
	Enabled  bool         // off by default; the user must opt in
	Endpoint string       // own-site ingest URL
	HTTP     *http.Client // nil → a 5s-timeout default
	Warn     func(string) // loud warning sink; nil → discarded (tests inject)

	warned bool // WARN-once latch
}

// Preview returns the exact bytes Send would transmit. The config screen shows
// this before the user enables telemetry, so "what is sent" is inspectable and
// equals what the server receives.
func (c *Client) Preview(p Payload) ([]byte, error) {
	return p.Marshal()
}

// Send transmits p if and only if the client is enabled. When disabled it is a
// no-op and makes NO network call. On a transport/endpoint failure it warns once
// and returns the error (no retry).
func (c *Client) Send(p Payload) error {
	if !c.Enabled {
		return nil // off by default — zero network
	}
	body, err := p.Marshal()
	if err != nil {
		return err
	}
	resp, err := c.client().Post(c.Endpoint, "application/json", bytes.NewReader(body))
	if err != nil {
		c.warnOnce("telemetry endpoint unreachable: " + err.Error())
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusNoContent {
		c.warnOnce(fmt.Sprintf("telemetry endpoint returned %d", resp.StatusCode))
		return fmt.Errorf("telemetry status %d", resp.StatusCode)
	}
	return nil
}

func (c *Client) client() *http.Client {
	if c.HTTP != nil {
		return c.HTTP
	}
	return &http.Client{Timeout: 5 * time.Second}
}

func (c *Client) warnOnce(msg string) {
	if c.warned {
		return
	}
	c.warned = true
	if c.Warn != nil {
		c.Warn(msg)
	}
}
