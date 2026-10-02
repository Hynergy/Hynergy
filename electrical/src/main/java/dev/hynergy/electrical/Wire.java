package dev.hynergy.electrical;

/**
 * Provides a runtime handle to a wire in an {@link ElectricalSystem}.
 *
 * <p>A wire connects device terminals to the same electrical network.
 * Each handle is bound to the electrical system that created or resolved it
 * and must not be used with another system.</p>
 *
 * <p>The {@link WireId} identifies the underlying wire independently of this
 * Java handle. Persist a wire ID rather than a {@code Wire}; the handle itself
 * is valid only for the lifetime of its electrical system. After an electrical
 * world is restored, a new handle can be obtained for the persisted wire ID.</p>
 */
public final class Wire {
    private final ElectricalSystem system;
    private final WireId wireId;

    Wire(ElectricalSystem system, WireId wireId) {
        this.system = system;
        this.wireId = wireId;
    }

    public void connect(Wire other) {
        system.connect(this, other);
    }

    public void disconnect(Wire other) {
        system.disconnect(this, other);
    }

    public void destroy() {
        system.remove(this);
    }
    
    /**
     * Returns the persistent identity of this wire.
     *
     * @return the wire ID
     */
    public WireId id() {
        return wireId;
    }


    boolean belongsTo(ElectricalSystem system) {
        return this.system == system;
    }
}
