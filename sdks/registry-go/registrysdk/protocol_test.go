package registrysdk

import (
	"encoding/json"
	"testing"
)

func TestUnitVariantEncoding(t *testing.T) {
	payload, err := json.Marshal("Ping")
	if err != nil {
		t.Fatalf("marshal ping failed: %v", err)
	}
	if string(payload) != "\"Ping\"" {
		t.Fatalf("expected unit variant string, got %s", string(payload))
	}
}

func TestMetadataDefault(t *testing.T) {
	client, err := NewClient(
		Config{MainAddress: "127.0.0.1:7946", ClusterToken: "t"},
		Registration{
			NodeID:    "n1",
			Endpoints: []Endpoint{{Protocol: "http", Address: "10.0.1.2:8080"}},
			Metadata:  map[string]string{},
		},
	)
	if err != nil {
		t.Fatalf("new client failed: %v", err)
	}
	if got := client.reg.Metadata["meta.schema"]; got != "rinfra.meta.v1" {
		t.Fatalf("expected meta.schema default, got %q", got)
	}
}
