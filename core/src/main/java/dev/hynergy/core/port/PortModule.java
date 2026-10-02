package dev.hynergy.core.port;

import dev.hynergy.core.HynergyModule;

import java.util.Objects;

/**
 * Registration and discovery entry point for Hynergy ports.
 *
 * <p>Protocol registration (domains, standards, adapters) is frozen when the
 * module starts. Block port definitions are runtime data and may be installed,
 * replaced, or removed afterwards, which allows an asset loader to own them.</p>
 */
public final class PortModule extends HynergyModule {
    private final PortRegistry registry = new PortRegistry();
    private final RuntimePortDefinitions blockPorts = new RuntimePortDefinitions();

    private PortDiscovery discovery;

    @Override
    protected void setup() {
    }

    @Override
    protected void start() {
        freeze();
    }

    public <R> PortDomain<R> registerDomain(String id) {
        return registry.registerDomain(id);
    }

    public <P, R> PortStandard<P, R> registerStandard(
        String id,
        PortDomain<R> domain,
        Class<P> profileType,
        PortResolver<P, P, R> resolver
    ) {
        return registry.registerStandard(id, domain, profileType, resolver);
    }

    public <A, B, R> void registerAdapter(
        PortStandard<A, R> first,
        PortStandard<B, R> second,
        PortResolver<A, B, R> resolver
    ) {
        registry.registerAdapter(first, second, resolver);
    }

    /**
     * Installs or replaces the runtime port layout for one block type.
     *
     * <p>This method is intentionally available both before and after protocol
     * freeze so a future Hytale asset codec/reload path can publish definitions
     * without mutating the protocol registry.</p>
     */
    public void setBlockPorts(int blockTypeId, BlockPortDefinition definition) {
        Objects.requireNonNull(definition, "definition");
        for (int index = 0; index < definition.size(); index++) {
            PortStandard<?, ?> standard = definition.portAt(index).standard();
            if (!registry.owns(standard)) {
                throw new IllegalArgumentException(
                    "Block port definition references a standard from another PortModule: " + standard.id()
                );
            }
        }
        blockPorts.set(blockTypeId, definition);
    }

    /** Removes the current runtime port layout for one block type. */
    public void clearBlockPorts(int blockTypeId) {
        blockPorts.clear(blockTypeId);
    }

    /** Returns the current runtime definition, or {@code null} when none is installed. */
    public BlockPortDefinition blockPorts(int blockTypeId) {
        return blockPorts.get(blockTypeId);
    }

    /** Looks up a registered protocol standard by namespaced ID for asset codecs. */
    public PortStandard<?, ?> standard(String id) {
        return registry.standard(Objects.requireNonNull(id, "id"));
    }

    public PortDiscovery discovery() {
        PortDiscovery discovery = this.discovery;
        if (discovery == null) {
            throw new IllegalStateException("Port discovery is unavailable before the port module starts");
        }
        return discovery;
    }

    void freezeForTest() {
        freeze();
    }

    private void freeze() {
        registry.freeze();
        discovery = new PortDiscovery(registry, blockPorts);
    }
}
