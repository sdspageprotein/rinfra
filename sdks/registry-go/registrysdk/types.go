package registrysdk

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

type ConnectionState string

const (
	StateDisconnected ConnectionState = "DISCONNECTED"
	StateConnecting   ConnectionState = "CONNECTING"
	StateRegistering  ConnectionState = "REGISTERING"
	StateConnected    ConnectionState = "CONNECTED"
	StateReconnecting ConnectionState = "RECONNECTING"
	StateStopping     ConnectionState = "STOPPING"
)

type Endpoint struct {
	Protocol string `json:"protocol"`
	Address  string `json:"address"`
}

type Registration struct {
	NodeID    string            `json:"-"`
	Endpoints []Endpoint        `json:"-"`
	Metadata  map[string]string `json:"-"`
}

type Config struct {
	MainAddress      string
	ClusterToken     string
	PingInterval     time.Duration
	RegisterTimeout  time.Duration
	InitialReconnect time.Duration
	MaxReconnect     time.Duration
}

func (c *Config) fillDefaults() {
	if c.PingInterval <= 0 {
		c.PingInterval = 10 * time.Second
	}
	if c.RegisterTimeout <= 0 {
		c.RegisterTimeout = 5 * time.Second
	}
	if c.InitialReconnect <= 0 {
		c.InitialReconnect = 3 * time.Second
	}
	if c.MaxReconnect <= 0 {
		c.MaxReconnect = 30 * time.Second
	}
}

func validateRegistration(reg Registration) error {
	if strings.TrimSpace(reg.NodeID) == "" {
		return errors.New("node_id must not be empty")
	}
	if len(reg.Endpoints) == 0 {
		return errors.New("endpoints must not be empty")
	}
	for _, endpoint := range reg.Endpoints {
		if strings.TrimSpace(endpoint.Protocol) == "" {
			return errors.New("endpoint protocol must not be empty")
		}
		if !strings.Contains(endpoint.Address, ":") {
			return fmt.Errorf("endpoint address must be host:port: %s", endpoint.Address)
		}
	}
	return nil
}
