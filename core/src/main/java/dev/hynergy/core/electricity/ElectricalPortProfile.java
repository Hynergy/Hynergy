package dev.hynergy.core.electricity;

/**
 * Runtime compatibility data for one direct electrical conductor port.
 *
 * <p>The normal is block-local and is transformed by port discovery using the
 * owning block's exact rotation.</p>
 *
 * @param normalX outward cardinal normal X component
 * @param normalY outward cardinal normal Y component
 * @param normalZ outward cardinal normal Z component
 */
public record ElectricalPortProfile(int normalX, int normalY, int normalZ) {
    public ElectricalPortProfile {
        if (normalX < -1 || normalX > 1
                || normalY < -1 || normalY > 1
                || normalZ < -1 || normalZ > 1
                || Math.abs(normalX) + Math.abs(normalY) + Math.abs(normalZ) != 1) {
            throw new IllegalArgumentException("Electrical port normal must be one unit cardinal vector");
        }
    }
}
