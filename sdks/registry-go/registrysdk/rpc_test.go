package registrysdk

import (
	"context"
	"errors"
	"testing"
)

type fakeProvider struct {
	nodes []NodeInfo
	err   error
}

func (p fakeProvider) ListNodes(_ context.Context) ([]NodeInfo, error) {
	return p.nodes, p.err
}

type fakeCaller struct {
	err     error
	calls   int
	failFor int
}

func (c *fakeCaller) InvokeUnary(_ context.Context, _ Endpoint, _ string, _ any, _ any) error {
	c.calls++
	if c.failFor > 0 && c.calls <= c.failFor {
		return c.err
	}
	return nil
}

func TestResolverFiltersProtocolAndMetadata(t *testing.T) {
	resolver := NewResolver(fakeProvider{
		nodes: []NodeInfo{
			{
				ID: "n1",
				Endpoints: []Endpoint{
					{Protocol: "http", Address: "10.0.0.1:8080"},
					{Protocol: "grpc", Address: "10.0.0.1:9090"},
				},
				Metadata: map[string]string{
					"service.name":    "order",
					"service.version": "v1",
				},
			},
		},
	})
	eps, err := resolver.List(context.Background(), ResolveOptions{
		Service:        "order",
		ServiceVersion: "v1",
		Protocol:       "grpc",
	})
	if err != nil {
		t.Fatalf("resolve failed: %v", err)
	}
	if len(eps) != 1 || eps[0].Address != "10.0.0.1:9090" {
		t.Fatalf("unexpected resolved endpoints: %+v", eps)
	}
}

func TestResolverReturnsNotFound(t *testing.T) {
	resolver := NewResolver(fakeProvider{
		nodes: []NodeInfo{
			{
				ID:        "n1",
				Endpoints: []Endpoint{{Protocol: "http", Address: "10.0.0.1:8080"}},
				Metadata:  map[string]string{"service.name": "order"},
			},
		},
	})
	_, err := resolver.Resolve(context.Background(), ResolveOptions{Service: "order"})
	var rpcErr *RpcError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("expected RpcError, got %T", err)
	}
	if rpcErr.Code != RpcErrorCodeNotFound {
		t.Fatalf("expected not_found, got %s", rpcErr.Code)
	}
}

func TestRpcInvokerRetryOnUnavailable(t *testing.T) {
	resolver := NewResolver(fakeProvider{
		nodes: []NodeInfo{
			{
				ID:        "n1",
				Endpoints: []Endpoint{{Protocol: "grpc", Address: "10.0.0.1:9090"}},
				Metadata:  map[string]string{"service.name": "order"},
			},
		},
	})
	caller := &fakeCaller{
		err:     errors.New("rpc error: code = Unavailable desc = temporary unavailable"),
		failFor: 2,
	}
	invoker := NewRpcInvoker(resolver, caller)
	err := invoker.InvokeUnaryByService(
		context.Background(),
		ResolveOptions{Service: "order"},
		"/order.v1.OrderService/GetOrder",
		map[string]string{"id": "1"},
		new(map[string]any),
		CallOptions{
			Retry: RetryPolicy{
				MaxAttempts: 3,
			},
		},
	)
	if err != nil {
		t.Fatalf("expected success after retries, got %v", err)
	}
	if caller.calls != 3 {
		t.Fatalf("expected 3 calls, got %d", caller.calls)
	}
}
