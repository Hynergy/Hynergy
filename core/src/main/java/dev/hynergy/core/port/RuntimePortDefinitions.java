package dev.hynergy.core.port;

import java.util.Objects;
import java.util.concurrent.atomic.AtomicReferenceArray;

final class RuntimePortDefinitions {
    private volatile AtomicReferenceArray<BlockPortDefinition> definitions = new AtomicReferenceArray<>(16);

    BlockPortDefinition get(int blockTypeId) {
        if (blockTypeId < 0) {
            return null;
        }
        AtomicReferenceArray<BlockPortDefinition> current = definitions;
        return blockTypeId < current.length() ? current.get(blockTypeId) : null;
    }

    synchronized void set(int blockTypeId, BlockPortDefinition definition) {
        if (blockTypeId < 0) {
            throw new IllegalArgumentException("blockTypeId must be non-negative");
        }
        Objects.requireNonNull(definition, "definition");

        AtomicReferenceArray<BlockPortDefinition> current = definitions;
        if (blockTypeId >= current.length()) {
            int newLength = current.length();
            while (newLength <= blockTypeId) {
                newLength = Math.multiplyExact(newLength, 2);
            }
            AtomicReferenceArray<BlockPortDefinition> grown = new AtomicReferenceArray<>(newLength);
            for (int index = 0; index < current.length(); index++) {
                grown.set(index, current.get(index));
            }
            definitions = grown;
            current = grown;
        }
        current.set(blockTypeId, definition);
    }

    synchronized void clear(int blockTypeId) {
        if (blockTypeId < 0) {
            throw new IllegalArgumentException("blockTypeId must be non-negative");
        }
        AtomicReferenceArray<BlockPortDefinition> current = definitions;
        if (blockTypeId < current.length()) {
            current.set(blockTypeId, null);
        }
    }
}
