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

type NodeInfo struct {
	ID        string            `json:"id"`
	Endpoints []Endpoint        `json:"endpoints"`
	Metadata  map[string]string `json:"metadata"`
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

type RpcErrorCode string

const (
	RpcErrorCodeTimeout          RpcErrorCode = "timeout"
	RpcErrorCodeUnavailable      RpcErrorCode = "unavailable"
	RpcErrorCodeNotFound         RpcErrorCode = "not_found"
	RpcErrorCodeInvalidArgument  RpcErrorCode = "invalid_argument"
	RpcErrorCodeInternal         RpcErrorCode = "internal"
	RpcErrorCodeUnauthenticated  RpcErrorCode = "unauthenticated"
	RpcErrorCodePermissionDenied RpcErrorCode = "permission_denied"
	RpcErrorCodeCancelled        RpcErrorCode = "cancelled"
	RpcErrorCodeUnknown          RpcErrorCode = "unknown"
)

type RpcError struct {
	Code    RpcErrorCode
	Message string
	Cause   error
	Service string
	Method  string
}

func (e *RpcError) Error() string {
	if e == nil {
		return ""
	}
	if e.Cause == nil {
		return e.Message
	}
	return fmt.Sprintf("%s: %v", e.Message, e.Cause)
}

func (e *RpcError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Cause
}

type RetryPolicy struct {
	MaxAttempts int
	BaseDelay   time.Duration
	MaxDelay    time.Duration
	Jitter      bool
	RetryOn     map[RpcErrorCode]bool
}

func (p RetryPolicy) withDefaults() RetryPolicy {
	if p.MaxAttempts <= 0 {
		p.MaxAttempts = 3
	}
	if p.BaseDelay <= 0 {
		p.BaseDelay = 100 * time.Millisecond
	}
	if p.MaxDelay <= 0 {
		p.MaxDelay = time.Second
	}
	if len(p.RetryOn) == 0 {
		p.RetryOn = map[RpcErrorCode]bool{
			RpcErrorCodeUnavailable: true,
			RpcErrorCodeTimeout:     true,
		}
	}
	return p
}

type ResolveOptions struct {
	Protocol       string
	Service        string
	ServiceVersion string
	Zone           string
	PreferHealthy  bool
}

func (o ResolveOptions) withDefaults() ResolveOptions {
	if strings.TrimSpace(o.Protocol) == "" {
		o.Protocol = "grpc"
	}
	return o
}

type CallOptions struct {
	Timeout time.Duration
	Retry   RetryPolicy
	Headers map[string]string
}

func (o CallOptions) withDefaults() CallOptions {
	if o.Timeout <= 0 {
		o.Timeout = 2 * time.Second
	}
	o.Retry = o.Retry.withDefaults()
	return o
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
