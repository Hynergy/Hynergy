package dev.hynergy.core.electricity;

/**
 * Asset/runtime data for one direct electrical conductor port.
 *
 * <p>The terminal ID maps the physical block port to one fixed terminal in the
 * electrical device definition. The normal is block-local and is transformed by
 * port discovery using the owning block's exact rotation.</p>
 *
 * @param terminalId electrical device terminal index represented by this port
 * @param normalX outward cardinal normal X component
 * @param normalY outward cardinal normal Y component
 * @param normalZ outward cardinal normal Z component
 */
public record ElectricalPortProfile(int terminalId, int normalX, int normalY, int normalZ) {
    public ElectricalPortProfile {
        if (terminalId < 0) {
            throw new IllegalArgumentException("Electrical terminal ID must be non-negative");
        }
        if (normalX < -1 || normalX > 1
            || normalY < -1 || normalY > 1
            || normalZ < -1 || normalZ > 1
            || Math.abs(normalX) + Math.abs(normalY) + Math.abs(normalZ) != 1) {
            throw new IllegalArgumentException("Electrical port normal must be one unit cardinal vector");
        }
    }
}
