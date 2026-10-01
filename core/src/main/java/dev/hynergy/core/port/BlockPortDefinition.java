package dev.hynergy.core.port;

import java.util.Arrays;
import java.util.Objects;

/**
 * Immutable port layout for one runtime block type.
 *
 * <p>This is data, not connection state. Asset loaders may replace the definition
 * at runtime; subsequent discovery observes the replacement.</p>
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

    int size() {
        return ports.length;
    }

    PortDefinition<?, ?> portAt(int index) {
        return ports[index];
    }

    @Override
    public String toString() {
        return "BlockPortDefinition" + Arrays.toString(ports);
    }
}
