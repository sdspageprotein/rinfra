package io.rinfra.registry.sdk;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.fasterxml.jackson.databind.node.TextNode;
import io.netty.bootstrap.Bootstrap;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.Channel;
import io.netty.channel.ChannelFuture;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInitializer;
import io.netty.channel.ChannelPipeline;
import io.netty.channel.SimpleChannelInboundHandler;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.SocketChannel;
import io.netty.channel.socket.nio.NioSocketChannel;
import io.netty.handler.codec.LengthFieldBasedFrameDecoder;
import io.netty.handler.codec.LengthFieldPrepender;
import io.netty.util.CharsetUtil;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Random;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

public final class RegistryClient {
    private static final Logger log = LoggerFactory.getLogger(RegistryClient.class);

    private final RegistryClientConfig config;
    private final Registration registration;
    private final ObjectMapper mapper = new ObjectMapper();
    private final BlockingQueue<JsonNode> inbox = new LinkedBlockingQueue<>();
    private final ScheduledExecutorService scheduler = Executors.newSingleThreadScheduledExecutor();
    private final AtomicReference<ConnectionState> state = new AtomicReference<>(ConnectionState.DISCONNECTED);
    private final Random random = new Random();

    private NioEventLoopGroup group;
    private volatile Channel channel;
    private volatile boolean running;
    private volatile boolean authFailed;
    private volatile long reconnectDelaySeconds = 3;

    public RegistryClient(RegistryClientConfig config, Registration registration) {
        this.config = Objects.requireNonNull(config, "config");
        this.registration = validateRegistration(Objects.requireNonNull(registration, "registration"));
    }

    public synchronized void start() {
        if (running) {
            return;
        }
        running = true;
        authFailed = false;
        reconnectDelaySeconds = 3;
        group = new NioEventLoopGroup(1);
        connectAndRegister(true);
    }

    public synchronized void stop() {
        running = false;
        state.set(ConnectionState.STOPPING);
        sendDeregisterBestEffort();
        closeChannel();
        if (group != null) {
            group.shutdownGracefully();
            group = null;
        }
        scheduler.shutdownNow();
        state.set(ConnectionState.DISCONNECTED);
    }

    public ConnectionState state() {
        return state.get();
    }

    public List<JsonNode> listNodes() {
        ensureConnected();
        sendMessage(buildListNodesMessage());
        JsonNode nodeList = awaitMessage("NodeList", Duration.ofSeconds(5));
        JsonNode nodes = nodeList.path("nodes");
        List<JsonNode> result = new ArrayList<>();
        if (nodes.isArray()) {
            nodes.forEach(result::add);
        }
        return result;
    }

    private void connectAndRegister(boolean initial) {
        if (!running || authFailed) {
            return;
        }
        state.set(initial ? ConnectionState.CONNECTING : ConnectionState.RECONNECTING);

        HostPort hostPort = HostPort.parse(config.mainAddress());
        Bootstrap bootstrap = new Bootstrap()
                .group(group)
                .channel(NioSocketChannel.class)
                .handler(new ChannelInitializer<SocketChannel>() {
                    @Override
                    protected void initChannel(SocketChannel ch) {
                        ChannelPipeline pipeline = ch.pipeline();
                        pipeline.addLast(new LengthFieldBasedFrameDecoder(64 * 1024, 0, 4, 0, 4));
                        pipeline.addLast(new LengthFieldPrepender(4));
                        pipeline.addLast(new InboundHandler());
                    }
                });

        ChannelFuture future = bootstrap.connect(hostPort.host(), hostPort.port());
        future.addListener(f -> {
            if (!f.isSuccess()) {
                scheduleReconnect();
                return;
            }

            channel = future.channel();
            try {
                state.set(ConnectionState.REGISTERING);
                sendMessage(buildRegisterMessage());
                JsonNode ack = awaitMessage("RegisterAck", config.registerTimeout());
                boolean success = ack.path("success").asBoolean(false);
                if (!success) {
                    authFailed = true;
                    running = false;
                    String err = ack.path("error").isNull() ? "register rejected" : ack.path("error").asText();
                    closeChannel();
                    throw new IllegalStateException("register failed: " + err);
                }
                reconnectDelaySeconds = 3;
                state.set(ConnectionState.CONNECTED);
                schedulePing();
            } catch (RuntimeException e) {
                log.warn("register failed: {}", e.getMessage());
                if (!authFailed) {
                    scheduleReconnect();
                }
            }
        });
    }

    private void schedulePing() {
        long intervalMs = Math.max(500, config.pingInterval().toMillis());
        scheduler.scheduleAtFixedRate(() -> {
            if (!running || state.get() != ConnectionState.CONNECTED) {
                return;
            }
            sendMessage(buildPingMessage());
        }, intervalMs, intervalMs, TimeUnit.MILLISECONDS);
    }

    private void scheduleReconnect() {
        if (!running || authFailed) {
            return;
        }
        state.set(ConnectionState.RECONNECTING);
        long base = reconnectDelaySeconds;
        long jitter = Math.max(1, (long) (base * (0.1 + (0.1 * random.nextDouble()))));
        long delay = Math.min(30, base + jitter);
        reconnectDelaySeconds = Math.min(30, base * 2);
        scheduler.schedule(() -> connectAndRegister(false), delay, TimeUnit.SECONDS);
    }

    private void ensureConnected() {
        if (state.get() != ConnectionState.CONNECTED || channel == null || !channel.isActive()) {
            throw new IllegalStateException("registry client is not connected");
        }
    }

    private void sendDeregisterBestEffort() {
        if (channel == null || !channel.isActive()) {
            return;
        }
        try {
            sendMessage(buildDeregisterMessage());
        } catch (Exception ignored) {
            // Best effort during shutdown.
        }
    }

    private void closeChannel() {
        Channel ch = channel;
        channel = null;
        if (ch != null) {
            ch.close();
        }
    }

    private void sendMessage(JsonNode payload) {
        Channel ch = channel;
        if (ch == null || !ch.isActive()) {
            return;
        }
        try {
            byte[] bytes = mapper.writeValueAsBytes(payload);
            ByteBuf buf = Unpooled.wrappedBuffer(bytes);
            ch.writeAndFlush(buf);
        } catch (JsonProcessingException e) {
            throw new IllegalStateException("failed to encode message", e);
        }
    }

    private JsonNode awaitMessage(String variant, Duration timeout) {
        long deadline = System.currentTimeMillis() + timeout.toMillis();
        while (System.currentTimeMillis() < deadline) {
            try {
                JsonNode msg = inbox.poll(100, TimeUnit.MILLISECONDS);
                if (msg == null || !msg.isObject() || !msg.has(variant)) {
                    continue;
                }
                return msg.get(variant);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException("interrupted while waiting message", e);
            }
        }
        throw new IllegalStateException("timeout waiting message: " + variant);
    }

    private Registration validateRegistration(Registration reg) {
        if (reg.endpoints().isEmpty()) {
            throw new IllegalArgumentException("endpoints must not be empty");
        }
        for (Endpoint endpoint : reg.endpoints()) {
            if (endpoint.protocol().isBlank()) {
                throw new IllegalArgumentException("endpoint protocol must not be blank");
            }
            if (endpoint.address().isBlank() || !endpoint.address().contains(":")) {
                throw new IllegalArgumentException("endpoint address must be host:port");
            }
        }
        return reg;
    }

    private JsonNode buildRegisterMessage() {
        ObjectNode register = mapper.createObjectNode();
        register.put("node_id", registration.nodeId());
        register.put("role", "Worker");
        ArrayNode endpointsNode = register.putArray("endpoints");
        for (Endpoint endpoint : registration.endpoints()) {
            ObjectNode e = mapper.createObjectNode();
            e.put("protocol", endpoint.protocol());
            e.put("address", endpoint.address());
            endpointsNode.add(e);
        }
        ObjectNode metadataNode = register.putObject("metadata");
        for (Map.Entry<String, String> entry : registration.metadata().entrySet()) {
            metadataNode.put(entry.getKey(), entry.getValue());
        }
        if (!metadataNode.has("meta.schema")) {
            metadataNode.put("meta.schema", "rinfra.meta.v1");
        }
        register.put("token", config.clusterToken());
        register.putNull("trace_context");

        ObjectNode root = mapper.createObjectNode();
        root.set("Register", register);
        return root;
    }

    private JsonNode buildDeregisterMessage() {
        ObjectNode deregister = mapper.createObjectNode();
        deregister.put("node_id", registration.nodeId());
        deregister.putNull("trace_context");
        ObjectNode root = mapper.createObjectNode();
        root.set("Deregister", deregister);
        return root;
    }

    private JsonNode buildListNodesMessage() {
        return TextNode.valueOf("ListNodes");
    }

    private JsonNode buildPingMessage() {
        return TextNode.valueOf("Ping");
    }

    private JsonNode buildPongMessage() {
        return TextNode.valueOf("Pong");
    }

    private final class InboundHandler extends SimpleChannelInboundHandler<ByteBuf> {
        @Override
        protected void channelRead0(ChannelHandlerContext ctx, ByteBuf msg) throws Exception {
            String json = msg.toString(CharsetUtil.UTF_8);
            JsonNode root = mapper.readTree(json);
            if (root.isTextual() && "Ping".equals(root.asText())) {
                sendMessage(buildPongMessage());
                return;
            }
            if (root.isTextual() && "Pong".equals(root.asText())) {
                return;
            }
            inbox.offer(root);
        }

        @Override
        public void channelInactive(ChannelHandlerContext ctx) {
            if (running && !authFailed && state.get() != ConnectionState.STOPPING) {
                scheduleReconnect();
            }
        }

        @Override
        public void exceptionCaught(ChannelHandlerContext ctx, Throwable cause) {
            log.warn("netty pipeline error: {}", cause.getMessage());
            ctx.close();
        }
    }

    private record HostPort(String host, int port) {
        private static HostPort parse(String address) {
            int idx = address.lastIndexOf(':');
            if (idx <= 0 || idx >= address.length() - 1) {
                throw new IllegalArgumentException("mainAddress must be host:port");
            }
            String host = address.substring(0, idx);
            int port = Integer.parseInt(address.substring(idx + 1));
            return new HostPort(host, port);
        }
    }
}
