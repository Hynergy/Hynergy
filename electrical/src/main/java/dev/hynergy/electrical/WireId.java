package dev.hynergy.electrical;

/**
 * Persistently identifies one wire in an electrical world.
 *
 * <p>A wire ID remains associated with the same logical wire when the
 * electrical world is serialized and restored. The generation distinguishes
 * different wires that reuse the same numeric ID.</p>
 *
 * <p>Wire IDs are scoped to an electrical world. An ID from one world must
 * not be used to identify a wire in another world.</p>
 *
 * <p>A wire ID does not provide access to the wire by itself. A live
 * {@link Wire} handle is required to interact with the wire at runtime.</p>
 *
 * @param value      the non-zero wire ID
 * @param generation the generation of the wire ID
 */
public record WireId(int value, int generation) {

    public long packed() {
        return ((long) value << 32)
                | Integer.toUnsignedLong(generation);
    }

    public static WireId fromPacked(long packed) {
        return new WireId(
                (int) (packed >>> 32),
                (int) packed
        );
    }

}
