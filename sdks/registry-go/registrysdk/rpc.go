package registrysdk

import (
	"context"
	"encoding/json"
	"errors"
	"math/rand"
	"strings"
	"time"
)

type NodeProvider interface {
	ListNodes(ctx context.Context) ([]NodeInfo, error)
}

type Resolver struct {
	provider NodeProvider
}

func NewResolver(provider NodeProvider) *Resolver {
	return &Resolver{provider: provider}
}

func (r *Resolver) List(ctx context.Context, opts ResolveOptions) ([]Endpoint, error) {
	if r == nil || r.provider == nil {
		return nil, &RpcError{
			Code:    RpcErrorCodeInternal,
			Message: "resolver provider is not configured",
		}
	}
	opts = opts.withDefaults()
	nodes, err := r.provider.ListNodes(ctx)
	if err != nil {
		return nil, &RpcError{
			Code:    RpcErrorCodeUnavailable,
			Message: "failed to list nodes",
			Cause:   err,
			Service: opts.Service,
		}
	}
	var candidates []Endpoint
	for _, node := range nodes {
		if !matchMetadata(node.Metadata, opts) {
			continue
		}
		for _, endpoint := range node.Endpoints {
			if strings.EqualFold(strings.TrimSpace(endpoint.Protocol), opts.Protocol) {
				candidates = append(candidates, endpoint)
			}
		}
	}
	if len(candidates) == 0 {
		return nil, &RpcError{
			Code:    RpcErrorCodeNotFound,
			Message: "no endpoint matched resolve filters",
			Service: opts.Service,
		}
	}
	return candidates, nil
}

func (r *Resolver) Resolve(ctx context.Context, opts ResolveOptions) (Endpoint, error) {
	endpoints, err := r.List(ctx, opts)
	if err != nil {
		return Endpoint{}, err
	}
	return endpoints[rand.Intn(len(endpoints))], nil
}

type UnaryCaller interface {
	InvokeUnary(ctx context.Context, endpoint Endpoint, method string, req any, resp any) error
}

type RpcInvoker struct {
	resolver *Resolver
	caller   UnaryCaller
}

func NewRpcInvoker(resolver *Resolver, caller UnaryCaller) *RpcInvoker {
	return &RpcInvoker{
		resolver: resolver,
		caller:   caller,
	}
}

func (i *RpcInvoker) InvokeUnary(
	ctx context.Context,
	endpoint Endpoint,
	method string,
	req any,
	resp any,
	callOpts CallOptions,
) error {
	if i == nil || i.caller == nil {
		return &RpcError{
			Code:    RpcErrorCodeInternal,
			Message: "rpc caller is not configured",
			Method:  method,
		}
	}
	callOpts = callOpts.withDefaults()
	rpcCtx, cancel := context.WithTimeout(ctx, callOpts.Timeout)
	defer cancel()
	err := i.caller.InvokeUnary(rpcCtx, endpoint, method, req, resp)
	if err == nil {
		return nil
	}
	return mapError(err, method, "")
}

func (i *RpcInvoker) InvokeUnaryByService(
	ctx context.Context,
	resolveOpts ResolveOptions,
	method string,
	req any,
	resp any,
	callOpts CallOptions,
) error {
	if i == nil || i.resolver == nil {
		return &RpcError{
			Code:    RpcErrorCodeInternal,
			Message: "resolver is not configured",
			Method:  method,
			Service: resolveOpts.Service,
		}
	}
	callOpts = callOpts.withDefaults()
	attempts := callOpts.Retry.MaxAttempts
	if attempts < 1 {
		attempts = 1
	}

	var lastErr error
	for attempt := 1; attempt <= attempts; attempt++ {
		endpoint, err := i.resolver.Resolve(ctx, resolveOpts)
		if err != nil {
			return err
		}
		err = i.InvokeUnary(ctx, endpoint, method, req, resp, callOpts)
		if err == nil {
			return nil
		}
		lastErr = err
		var rpcErr *RpcError
		if !errors.As(err, &rpcErr) || !callOpts.Retry.RetryOn[rpcErr.Code] || attempt == attempts {
			break
		}
		backoff := callOpts.Retry.BaseDelay * time.Duration(1<<(attempt-1))
		if backoff > callOpts.Retry.MaxDelay {
			backoff = callOpts.Retry.MaxDelay
		}
		if callOpts.Retry.Jitter {
			jitter := time.Duration(rand.Int63n(int64(backoff / 4)))
			backoff += jitter
		}
		select {
		case <-ctx.Done():
			return mapError(ctx.Err(), method, resolveOpts.Service)
		case <-time.After(backoff):
		}
	}
	if lastErr == nil {
		lastErr = &RpcError{
			Code:    RpcErrorCodeUnknown,
			Message: "rpc invocation failed",
			Method:  method,
			Service: resolveOpts.Service,
		}
	}
	return lastErr
}

type RegistryNodeProvider struct {
	client *Client
}

func NewRegistryNodeProvider(client *Client) *RegistryNodeProvider {
	return &RegistryNodeProvider{client: client}
}

func (p *RegistryNodeProvider) ListNodes(ctx context.Context) ([]NodeInfo, error) {
	if p == nil || p.client == nil {
		return nil, errors.New("registry client is not configured")
	}
	raw, err := p.client.ListNodes(ctx)
	if err != nil {
		return nil, err
	}
	nodes := make([]NodeInfo, 0, len(raw))
	for _, item := range raw {
		var node NodeInfo
		if unmarshalErr := json.Unmarshal(item, &node); unmarshalErr != nil {
			continue
		}
		if node.Metadata == nil {
			node.Metadata = map[string]string{}
		}
		nodes = append(nodes, node)
	}
	return nodes, nil
}

func matchMetadata(metadata map[string]string, opts ResolveOptions) bool {
	if strings.TrimSpace(opts.Service) != "" && metadata["service.name"] != opts.Service {
		return false
	}
	if strings.TrimSpace(opts.ServiceVersion) != "" && metadata["service.version"] != opts.ServiceVersion {
		return false
	}
	if strings.TrimSpace(opts.Zone) != "" && metadata["service.zone"] != opts.Zone {
		return false
	}
	return true
}

func mapError(err error, method string, service string) error {
	if err == nil {
		return nil
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return &RpcError{Code: RpcErrorCodeTimeout, Message: "rpc timeout", Cause: err, Method: method, Service: service}
	}
	if errors.Is(err, context.Canceled) {
		return &RpcError{Code: RpcErrorCodeCancelled, Message: "rpc canceled", Cause: err, Method: method, Service: service}
	}
	if grpcCode := inferGrpcCode(err); grpcCode != "" {
		return &RpcError{
			Code:    mapGrpcStatus(grpcCode),
			Message: err.Error(),
			Cause:   err,
			Method:  method,
			Service: service,
		}
	}
	var existing *RpcError
	if errors.As(err, &existing) {
		return existing
	}
	return &RpcError{
		Code:    RpcErrorCodeUnknown,
		Message: "rpc failed",
		Cause:   err,
		Method:  method,
		Service: service,
	}
}

func mapGrpcStatus(code string) RpcErrorCode {
	switch code {
	case "DeadlineExceeded":
		return RpcErrorCodeTimeout
	case "Unavailable":
		return RpcErrorCodeUnavailable
	case "NotFound":
		return RpcErrorCodeNotFound
	case "InvalidArgument":
		return RpcErrorCodeInvalidArgument
	case "Internal":
		return RpcErrorCodeInternal
	case "Unauthenticated":
		return RpcErrorCodeUnauthenticated
	case "PermissionDenied":
		return RpcErrorCodePermissionDenied
	case "Canceled":
		return RpcErrorCodeCancelled
	default:
		return RpcErrorCodeUnknown
	}
}

func inferGrpcCode(err error) string {
	text := err.Error()
	switch {
	case strings.Contains(text, "DeadlineExceeded"):
		return "DeadlineExceeded"
	case strings.Contains(text, "Unavailable"):
		return "Unavailable"
	case strings.Contains(text, "NotFound"):
		return "NotFound"
	case strings.Contains(text, "InvalidArgument"):
		return "InvalidArgument"
	case strings.Contains(text, "Internal"):
		return "Internal"
	case strings.Contains(text, "Unauthenticated"):
		return "Unauthenticated"
	case strings.Contains(text, "PermissionDenied"):
		return "PermissionDenied"
	case strings.Contains(text, "Canceled"):
		return "Canceled"
	default:
		return ""
	}
}
