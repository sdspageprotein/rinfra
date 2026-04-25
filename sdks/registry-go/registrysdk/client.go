package registrysdk

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand"
	"net"
	"sync"
	"sync/atomic"
	"time"
)

type nodeListMessage struct {
	NodeList struct {
		Nodes []json.RawMessage `json:"nodes"`
	} `json:"NodeList"`
}

type registerAckMessage struct {
	RegisterAck struct {
		Success bool    `json:"success"`
		Error   *string `json:"error"`
	} `json:"RegisterAck"`
}

type registerMessage struct {
	Register struct {
		NodeID       string            `json:"node_id"`
		Role         string            `json:"role"`
		Endpoints    []Endpoint        `json:"endpoints"`
		Metadata     map[string]string `json:"metadata"`
		Token        string            `json:"token"`
		TraceContext any               `json:"trace_context"`
	} `json:"Register"`
}

type deregisterMessage struct {
	Deregister struct {
		NodeID       string `json:"node_id"`
		TraceContext any    `json:"trace_context"`
	} `json:"Deregister"`
}

type Client struct {
	cfg Config
	reg Registration

	state atomic.Value

	mu            sync.Mutex
	conn          net.Conn
	running       bool
	authFailed    bool
	readerDone    chan struct{}
	reconnectDone chan struct{}
	pingDone      chan struct{}
	pongCh        chan struct{}
	nodeListCh    chan []json.RawMessage
	reconnectWait time.Duration
}

func NewClient(cfg Config, reg Registration) (*Client, error) {
	if err := validateRegistration(reg); err != nil {
		return nil, err
	}
	cfg.fillDefaults()
	if reg.Metadata == nil {
		reg.Metadata = make(map[string]string)
	}
	if _, ok := reg.Metadata["meta.schema"]; !ok {
		reg.Metadata["meta.schema"] = "rinfra.meta.v1"
	}

	c := &Client{
		cfg:           cfg,
		reg:           reg,
		readerDone:    make(chan struct{}),
		reconnectDone: make(chan struct{}),
		pingDone:      make(chan struct{}),
		pongCh:        make(chan struct{}, 1),
		nodeListCh:    make(chan []json.RawMessage, 1),
		reconnectWait: cfg.InitialReconnect,
	}
	c.state.Store(StateDisconnected)
	return c, nil
}

func (c *Client) State() ConnectionState {
	v := c.state.Load()
	if v == nil {
		return StateDisconnected
	}
	return v.(ConnectionState)
}

func (c *Client) Start(ctx context.Context) error {
	c.mu.Lock()
	if c.running {
		c.mu.Unlock()
		return nil
	}
	c.running = true
	c.authFailed = false
	c.reconnectWait = c.cfg.InitialReconnect
	c.readerDone = make(chan struct{})
	c.reconnectDone = make(chan struct{})
	c.pingDone = make(chan struct{})
	c.mu.Unlock()

	if err := c.connectAndRegister(ctx, true); err != nil {
		c.mu.Lock()
		c.running = false
		c.mu.Unlock()
		return err
	}

	go c.reconnectLoop()
	return nil
}

func (c *Client) Stop(ctx context.Context) error {
	c.setState(StateStopping)

	c.mu.Lock()
	if !c.running {
		c.mu.Unlock()
		c.setState(StateDisconnected)
		return nil
	}
	c.running = false
	conn := c.conn
	c.conn = nil
	pingDone := c.pingDone
	readerDone := c.readerDone
	reconnectDone := c.reconnectDone
	c.mu.Unlock()

	select {
	case <-pingDone:
	default:
		close(pingDone)
	}

	if conn != nil {
		_ = c.writeDeregister(conn)
		_ = conn.Close()
	}

	waitCtx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	for {
		select {
		case <-readerDone:
			readerDone = nil
		default:
		}
		select {
		case <-reconnectDone:
			reconnectDone = nil
		default:
		}
		if readerDone == nil && reconnectDone == nil {
			c.setState(StateDisconnected)
			return nil
		}
		select {
		case <-waitCtx.Done():
			c.setState(StateDisconnected)
			return nil
		case <-time.After(20 * time.Millisecond):
		}
	}
}

func (c *Client) ListNodes(ctx context.Context) ([]json.RawMessage, error) {
	conn, err := c.activeConn()
	if err != nil {
		return nil, err
	}
	if err := c.writeFrame(conn, "ListNodes"); err != nil {
		return nil, err
	}

	select {
	case nodes := <-c.nodeListCh:
		return nodes, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (c *Client) reconnectLoop() {
	defer close(c.reconnectDone)
	for {
		c.mu.Lock()
		running := c.running
		authFailed := c.authFailed
		wait := c.reconnectWait
		c.mu.Unlock()

		if !running || authFailed {
			return
		}

		if c.State() != StateConnected {
			c.setState(StateReconnecting)
			jitter := time.Duration(rand.Float64()*0.1*float64(wait)) + time.Duration(rand.Float64()*0.1*float64(wait))
			time.Sleep(wait + jitter)
			ctx, cancel := context.WithTimeout(context.Background(), c.cfg.RegisterTimeout)
			err := c.connectAndRegister(ctx, false)
			cancel()
			if err != nil {
				c.mu.Lock()
				c.reconnectWait *= 2
				if c.reconnectWait > c.cfg.MaxReconnect {
					c.reconnectWait = c.cfg.MaxReconnect
				}
				c.mu.Unlock()
				continue
			}
			c.mu.Lock()
			c.reconnectWait = c.cfg.InitialReconnect
			c.mu.Unlock()
		}
		time.Sleep(100 * time.Millisecond)
	}
}

func (c *Client) connectAndRegister(ctx context.Context, initial bool) error {
	if initial {
		c.setState(StateConnecting)
	} else {
		c.setState(StateReconnecting)
	}
	conn, err := net.DialTimeout("tcp", c.cfg.MainAddress, c.cfg.RegisterTimeout)
	if err != nil {
		return err
	}

	c.mu.Lock()
	if !c.running {
		c.mu.Unlock()
		_ = conn.Close()
		return errors.New("client stopped")
	}
	c.conn = conn
	c.readerDone = make(chan struct{})
	c.pingDone = make(chan struct{})
	c.pongCh = make(chan struct{}, 1)
	c.mu.Unlock()

	c.setState(StateRegistering)
	if err := c.writeRegister(conn); err != nil {
		_ = conn.Close()
		return err
	}

	ack, err := c.readRegisterAck(ctx, conn)
	if err != nil {
		_ = conn.Close()
		return err
	}
	if !ack.Success {
		c.mu.Lock()
		c.authFailed = true
		c.mu.Unlock()
		_ = conn.Close()
		if ack.Error != nil {
			return fmt.Errorf("register failed: %s", *ack.Error)
		}
		return errors.New("register failed")
	}

	c.setState(StateConnected)
	go c.readLoop(conn, c.readerDone, c.pingDone)
	go c.pingLoop(conn, c.pingDone)
	return nil
}

func (c *Client) readLoop(conn net.Conn, readerDone chan struct{}, pingDone chan struct{}) {
	defer close(readerDone)
	for {
		payload, err := c.readFrame(conn)
		if err != nil {
			if errors.Is(err, io.EOF) {
				c.handleDisconnect(conn, pingDone)
				return
			}
			c.handleDisconnect(conn, pingDone)
			return
		}

		var text string
		if err := json.Unmarshal(payload, &text); err == nil {
			switch text {
			case "Ping":
				_ = c.writeFrame(conn, "Pong")
			case "Pong":
				select {
				case c.pongCh <- struct{}{}:
				default:
				}
			}
			continue
		}

		var nl nodeListMessage
		if err := json.Unmarshal(payload, &nl); err == nil && len(nl.NodeList.Nodes) >= 0 {
			select {
			case c.nodeListCh <- nl.NodeList.Nodes:
			default:
				<-c.nodeListCh
				c.nodeListCh <- nl.NodeList.Nodes
			}
			continue
		}
	}
}

func (c *Client) pingLoop(conn net.Conn, done chan struct{}) {
	ticker := time.NewTicker(c.cfg.PingInterval)
	defer ticker.Stop()
	for {
		select {
		case <-done:
			return
		case <-ticker.C:
			if err := c.writeFrame(conn, "Ping"); err != nil {
				c.handleDisconnect(conn, done)
				return
			}
			select {
			case <-c.pongCh:
			case <-time.After(c.cfg.PingInterval):
				c.handleDisconnect(conn, done)
				return
			case <-done:
				return
			}
		}
	}
}

func (c *Client) handleDisconnect(conn net.Conn, pingDone chan struct{}) {
	c.mu.Lock()
	if c.conn == conn {
		c.conn = nil
	}
	running := c.running
	c.mu.Unlock()
	select {
	case <-pingDone:
	default:
		close(pingDone)
	}
	_ = conn.Close()
	if running && c.State() != StateStopping {
		c.setState(StateReconnecting)
	}
}

func (c *Client) activeConn() (net.Conn, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.conn == nil || c.State() != StateConnected {
		return nil, errors.New("registry client is not connected")
	}
	return c.conn, nil
}

func (c *Client) setState(state ConnectionState) {
	c.state.Store(state)
}

func (c *Client) writeRegister(conn net.Conn) error {
	msg := registerMessage{}
	msg.Register.NodeID = c.reg.NodeID
	msg.Register.Role = "Worker"
	msg.Register.Endpoints = c.reg.Endpoints
	msg.Register.Metadata = c.reg.Metadata
	msg.Register.Token = c.cfg.ClusterToken
	msg.Register.TraceContext = nil
	return c.writeFrame(conn, msg)
}

func (c *Client) writeDeregister(conn net.Conn) error {
	msg := deregisterMessage{}
	msg.Deregister.NodeID = c.reg.NodeID
	msg.Deregister.TraceContext = nil
	return c.writeFrame(conn, msg)
}

func (c *Client) readRegisterAck(ctx context.Context, conn net.Conn) (*struct {
	Success bool
	Error   *string
}, error) {
	type result struct {
		ack *struct {
			Success bool
			Error   *string
		}
		err error
	}
	ch := make(chan result, 1)
	go func() {
		payload, err := c.readFrame(conn)
		if err != nil {
			ch <- result{err: err}
			return
		}
		var ack registerAckMessage
		if err := json.Unmarshal(payload, &ack); err != nil {
			ch <- result{err: err}
			return
		}
		ch <- result{ack: &struct {
			Success bool
			Error   *string
		}{Success: ack.RegisterAck.Success, Error: ack.RegisterAck.Error}}
	}()
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case ret := <-ch:
		return ret.ack, ret.err
	}
}

func (c *Client) writeFrame(conn net.Conn, msg any) error {
	payload, err := json.Marshal(msg)
	if err != nil {
		return err
	}
	frame := make([]byte, 4+len(payload))
	binary.BigEndian.PutUint32(frame[:4], uint32(len(payload)))
	copy(frame[4:], payload)
	_, err = conn.Write(frame)
	return err
}

func (c *Client) readFrame(conn net.Conn) ([]byte, error) {
	header := make([]byte, 4)
	if _, err := io.ReadFull(conn, header); err != nil {
		return nil, err
	}
	size := binary.BigEndian.Uint32(header)
	if size > 64*1024 {
		return nil, fmt.Errorf("frame too large: %d", size)
	}
	payload := make([]byte, size)
	if _, err := io.ReadFull(conn, payload); err != nil {
		return nil, err
	}
	return payload, nil
}
