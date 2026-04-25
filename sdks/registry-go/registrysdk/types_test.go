package registrysdk

import "testing"

func TestValidateRegistration(t *testing.T) {
	err := validateRegistration(Registration{
		NodeID:    "n1",
		Endpoints: []Endpoint{{Protocol: "http", Address: "127.0.0.1:8080"}},
	})
	if err != nil {
		t.Fatalf("expected valid registration, got: %v", err)
	}
}

func TestValidateRegistrationEmptyEndpoints(t *testing.T) {
	err := validateRegistration(Registration{NodeID: "n1"})
	if err == nil {
		t.Fatalf("expected error for empty endpoints")
	}
}
