package dev.hynergy.core.port;

import java.util.Arrays;
import java.util.Objects;

/**
 * Stores a fixed port layout that blocks can share.
 *
 * <p>Each port must have a unique local ID within the layout.</p>
 */
public final class BlockPortDefinition {
    private final PortDefinition<?, ?>[] ports;

    private BlockPortDefinition(PortDefinition<?, ?>[] ports) {
        this.ports = ports;
    }

    public static BlockPortDefinition of(PortDefinition<?, ?>... ports) {
        Objects.requireNonNull(ports, "ports");
        PortDefinition<?, ?>[] copy = ports.clone();

        for (int index = 0; index < copy.length; index++) {
            Objects.requireNonNull(copy[index], "ports[" + index + "]");

            for (int previous = 0; previous < index; previous++) {
                if (copy[previous].localId() == copy[index].localId()) {
                    throw new IllegalArgumentException("Duplicate local port ID: " + copy[index].localId());
                }
            }
        }
        return new BlockPortDefinition(copy);
    }

    public PortDefinition<?, ?> port(int localId) {
        for (PortDefinition<?, ?> port : ports) {
            if (port.localId() == localId) {
                return port;
            }
        }
        return null;
    }

    public int size() {
        return ports.length;
    }

    public PortDefinition<?, ?> portAt(int index) {
        return ports[index];
    }

    @Override
    public String toString() {
        return "BlockPortDefinition" + Arrays.toString(ports);
    }
}
