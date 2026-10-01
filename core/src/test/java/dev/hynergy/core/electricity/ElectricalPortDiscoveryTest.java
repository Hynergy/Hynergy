package dev.hynergy.core.electricity;

import com.hypixel.hytale.component.ComponentRegistryProxy;
import com.hypixel.hytale.event.EventBus;
import com.hypixel.hytale.event.EventRegistry;
import com.hypixel.hytale.function.consumer.BooleanConsumer;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.Rotation;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import com.hypixel.hytale.server.core.plugin.registry.AssetRegistry;
import dev.hynergy.core.port.*;
import org.joml.Vector3i;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

final class ElectricalPortDiscoveryTest {

    @Test
    void opposingConductorFacesConnectDirectly() {
        Fixture fixture = Fixture.create(
                1, 0, 0, RotationTuple.NONE,
                -1, 0, 0, RotationTuple.NONE
        );

        assertEquals(List.of(ElectricalPortConnection.DIRECT), fixture.discover());
    }

    @Test
    void sameFacingConductorFacesDoNotConnect() {
        Fixture fixture = Fixture.create(
                1, 0, 0, RotationTuple.NONE,
                1, 0, 0, RotationTuple.NONE
        );

        assertTrue(fixture.discover().isEmpty());
    }

    @Test
    void perpendicularConductorFacesDoNotConnect() {
        Fixture fixture = Fixture.create(
                1, 0, 0, RotationTuple.NONE,
                0, 0, 1, RotationTuple.NONE
        );

        assertTrue(fixture.discover().isEmpty());
    }

    @Test
    void rotatedConductorUsesRotatedFace() {
        RotationTuple rotation = RotationTuple.of(Rotation.Ninety, Rotation.None);
        Vector3i offset = new Vector3i(1, 0, 0);
        rotation.applyRotationTo(offset);

        Fixture fixture = Fixture.create(
                1, 0, 0, rotation,
                -1, 0, 0, rotation,
                offset.x(), offset.y(), offset.z()
        );

        assertEquals(List.of(ElectricalPortConnection.DIRECT), fixture.discover());
    }

    @Test
    void conductorPortCarriesLocalIdStandardAndNormal() {
        PortModule ports = new PortModule();
        ElectricityModule electricity = createElectricityModule(ports);

        PortDefinition<ElectricalPortProfile, ElectricalPortConnection> port =
                electricity.conductorPort(7, PortOffset.ZERO, 1, 0, 0);

        assertEquals(7, port.localId());
        assertEquals(new ElectricalPortProfile(1, 0, 0), port.profile());
        assertEquals(electricity.conductorPortStandard(), port.standard());
    }

    @Test
    void pluginStandardCanRegisterAdapterToBuiltInConductorWithoutChangingElectricalModule() {
        PortModule ports = new PortModule();
        ElectricityModule electricity = createElectricityModule(ports);

        PortDomain<ElectricalPortConnection> domain = electricity.electricalPortDomain();
        record PluginProfile(int kind) {
        }
        PortStandard<PluginProfile, ElectricalPortConnection> plugin = ports.registerStandard(
                "plugin:connector",
                domain,
                PluginProfile.class,
                (first, second, geometry) -> null
        );
        ports.registerAdapter(
                plugin,
                electricity.conductorPortStandard(),
                (pluginProfile, conductor, geometry) -> ElectricalPortConnection.DIRECT
        );
        PortModuleTestAccess.freeze(ports);
        ports.setBlockPorts(1, BlockPortDefinition.of(
                new PortDefinition<>(
                        0,
                        PortOffset.ZERO,
                        PortReach.single(1, 0, 0),
                        plugin,
                        new PluginProfile(1)
                )
        ));
        ports.setBlockPorts(2, BlockPortDefinition.of(
                electricity.conductorPort(1, PortOffset.ZERO, -1, 0, 0)
        ));
        TestWorld world = new TestWorld();
        world.put(0, 0, 0, 1, RotationTuple.NONE);
        world.put(1, 0, 0, 2, RotationTuple.NONE);
        List<ElectricalPortConnection> results = new ArrayList<>();

        ports.discovery().discover(
                world, 0, 0, 0, 0, domain,
                (x, y, z, portId, result, sourceFirst) -> results.add(result)
        );

        assertEquals(List.of(ElectricalPortConnection.DIRECT), results);
    }

    @Test
    void profileRequiresOneUnitCardinalNormal() {
        assertThrows(IllegalArgumentException.class, () -> new ElectricalPortProfile(0, 0, 0));
        assertThrows(IllegalArgumentException.class, () -> new ElectricalPortProfile(1, 1, 0));
        assertThrows(IllegalArgumentException.class, () -> new ElectricalPortProfile(2, 0, 0));
    }

    private static ElectricityModule createElectricityModule(PortModule ports) {
        List<BooleanConsumer> registrations = new ArrayList<>();
        ElectricityModule electricity = new ElectricityModule(
                ports,
                new ComponentRegistryProxy<>(),
                new AssetRegistry(registrations),
                new EventRegistry(registrations, () -> true, null, new EventBus(false))
        );
        electricity.registerPortProtocols();
        return electricity;
    }

    private record Fixture(PortModule ports, PortDomain<ElectricalPortConnection> domain, TestWorld world) {

        static Fixture create(
                int sourceNormalX,
                int sourceNormalY,
                int sourceNormalZ,
                RotationTuple sourceRotation,
                int targetNormalX,
                int targetNormalY,
                int targetNormalZ,
                RotationTuple targetRotation
        ) {
            return create(
                    sourceNormalX, sourceNormalY, sourceNormalZ, sourceRotation,
                    targetNormalX, targetNormalY, targetNormalZ, targetRotation,
                    1, 0, 0
            );
        }

        static Fixture create(
                int sourceNormalX,
                int sourceNormalY,
                int sourceNormalZ,
                RotationTuple sourceRotation,
                int targetNormalX,
                int targetNormalY,
                int targetNormalZ,
                RotationTuple targetRotation,
                int targetX,
                int targetY,
                int targetZ
        ) {
            PortModule ports = new PortModule();
            ElectricityModule electricity = createElectricityModule(ports);
            PortModuleTestAccess.freeze(ports);
            ports.setBlockPorts(1, BlockPortDefinition.of(
                    electricity.conductorPort(
                            0, PortOffset.ZERO,
                            sourceNormalX, sourceNormalY, sourceNormalZ
                    )
            ));
            ports.setBlockPorts(2, BlockPortDefinition.of(
                    electricity.conductorPort(
                            1, PortOffset.ZERO,
                            targetNormalX, targetNormalY, targetNormalZ
                    )
            ));
            TestWorld world = new TestWorld();
            world.put(0, 0, 0, 1, sourceRotation);
            world.put(targetX, targetY, targetZ, 2, targetRotation);
            return new Fixture(ports, electricity.electricalPortDomain(), world);
        }

        List<ElectricalPortConnection> discover() {
            List<ElectricalPortConnection> results = new ArrayList<>();
            ports.discovery().discover(
                    world, 0, 0, 0, 0, domain,
                    (x, y, z, targetPortId, result, sourceFirst) -> results.add(result)
            );
            return results;
        }
    }

    private static final class TestWorld implements PortWorldView {
        private final Map<Position, Integer> ids = new HashMap<>();
        private final Map<Position, RotationTuple> rotations = new HashMap<>();

        void put(int x, int y, int z, int blockTypeId, RotationTuple rotation) {
            Position position = new Position(x, y, z);
            ids.put(position, blockTypeId);
            rotations.put(position, rotation);
        }

        @Override
        public int blockTypeId(int x, int y, int z) {
            return ids.getOrDefault(new Position(x, y, z), -1);
        }

        @Override
        public RotationTuple rotation(int x, int y, int z) {
            return rotations.getOrDefault(new Position(x, y, z), RotationTuple.NONE);
        }
    }

    private record Position(int x, int y, int z) {
    }
}
