package dev.hynergy.core.port;

import java.util.Arrays;
import java.util.Objects;

/**
 * Immutable bounded broad-phase reach for one logical port.
 *
 * <p>Offsets are block-local owner-to-owner candidate positions. Discovery rotates
 * them with the owning block before reading the world.</p>
 */
public final class PortReach {
    private final int[] offsets;

    private PortReach(int[] offsets) {
        this.offsets = offsets;
    }

    public static PortReach single(int x, int y, int z) {
        return new PortReach(new int[] {x, y, z});
    }

    public static PortReach of(PortOffset... offsets) {
        Objects.requireNonNull(offsets, "offsets");
        if (offsets.length == 0) {
            throw new IllegalArgumentException("A port reach must contain at least one candidate offset");
        }

        int[] packed = new int[offsets.length * 3];
        for (int index = 0; index < offsets.length; index++) {
            PortOffset offset = Objects.requireNonNull(offsets[index], "offsets[" + index + "]");
            int base = index * 3;
            packed[base] = offset.x();
            packed[base + 1] = offset.y();
            packed[base + 2] = offset.z();
        }
        return new PortReach(packed);
    }

    int size() {
        return offsets.length / 3;
    }

    int x(int index) {
        return offsets[index * 3];
    }

    int y(int index) {
        return offsets[index * 3 + 1];
    }

    int z(int index) {
        return offsets[index * 3 + 2];
    }

    @Override
    public boolean equals(Object object) {
        return object instanceof PortReach other && Arrays.equals(offsets, other.offsets);
    }

    @Override
    public int hashCode() {
        return Arrays.hashCode(offsets);
    }
}
