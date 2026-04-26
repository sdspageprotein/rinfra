package io.rinfra.registry.sdk;

import java.util.List;

@FunctionalInterface
public interface NodeProvider {
    List<NodeInfo> listNodes();
}
