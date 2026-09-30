package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class ElectricalWorldTest {

    @Test
    void wireIdsAreWorldAllocatedAndSequential() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int first = world.addWire();
            int second = world.addWire();

            assertEquals(1, first);
            assertEquals(2, second);

            world.applyCommands();
        }
    }

    @Test
    void deviceAndWireIdsUseIndependentNamespaces() {
        try (ElectricalEngine engine = ElectricalEngine.create();
            DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder();
            ElectricalWorld world = engine.createWorld(30)) {
            DeviceDefinition definition = engine.registerDefinition(builder);

            int wire = world.addWire();
            int device = world.addDevice(definition);

            assertEquals(1, wire);
            assertEquals(1, device);

            world.applyCommands();
        }
    }

    @Test
    void removedWireIsNotReusedBeforeBatchCommit() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int first = world.addWire();
            int second = world.addWire();

            int firstGeneration = world.wireGeneration(first);

            world.applyCommands();

            world.removeWire(first, firstGeneration);

            int third = world.addWire();

            assertEquals(3, third);

            world.applyCommands();

            int reused = world.addWire();

            assertEquals(first, reused);

            assertEquals(2, second);
        }
    }

    @Test
    void reusedWireRejectsStaleGeneration() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int id = world.addWire();

            int oldGeneration = world.wireGeneration(id);

            world.applyCommands();

            world.removeWire(id, oldGeneration);

            world.applyCommands();

            int reused = world.addWire();

            assertEquals(id, reused);

            int newGeneration = world.wireGeneration(reused);

            assertNotEquals(oldGeneration, newGeneration);

            assertThrows(IllegalStateException.class, () -> world.removeWire(reused, oldGeneration));
        }
    }

    @Test
    void sameBatchAddRemoveDoesNotAllowImmediateReuse() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int first = world.addWire();

            world.removeWire(first, world.wireGeneration(first));

            int second = world.addWire();

            assertEquals(2, second);

            world.applyCommands();

            assertEquals(first, world.addWire());
        }
    }

    @Test
    void pendingWireCanBeReferencedBeforeFlush() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int first = world.addWire();
            int second = world.addWire();

            world.connectWires(first, world.wireGeneration(first), second, world.wireGeneration(second));

            assertDoesNotThrow(world::applyCommands);
        }
    }

    @Test
    void removedWireBecomesImmediatelyUnusable() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int first = world.addWire();
            int second = world.addWire();

            int firstGeneration = world.wireGeneration(first);
            int secondGeneration = world.wireGeneration(second);

            world.applyCommands();

            world.removeWire(first, firstGeneration);

            assertThrows(
                IllegalStateException.class,
                () -> world.connectWires(first, firstGeneration, second, secondGeneration)
            );
        }
    }

    @Test
    void wireCannotBeConnectedToItself() {
        try (ElectricalEngine engine = ElectricalEngine.create();

            ElectricalWorld world = engine.createWorld(30)) {
            int wire = world.addWire();
            int generation = world.wireGeneration(wire);

            assertThrows(IllegalArgumentException.class, () -> world.connectWires(wire, generation, wire, generation));

            assertDoesNotThrow(world::applyCommands);
        }
    }

    @Test
    void unknownSubscriptionIsOwnershipConsistencyFailure() {
        try (ElectricalEngine engine = ElectricalEngine.create(); ElectricalWorld world = engine.createWorld(30)) {
            ElectricalWorld.SubscriptionOperationException failure =
                assertThrows(ElectricalWorld.SubscriptionOperationException.class, () -> world.unsubscribe(1));

            assertTrue(failure.isOwnershipConsistencyFailure());
        }
    }

    @Test
    void unknownObserverIsNotOwnershipConsistencyFailure() {
        try (ElectricalEngine engine = ElectricalEngine.create();
            DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder();
            ElectricalWorld world = engine.createWorld(30)) {
            DeviceDefinition definition = engine.registerDefinition(builder);

            int deviceId = world.addDevice(definition);
            int generation = world.deviceGeneration(deviceId);

            ElectricalWorld.SubscriptionOperationException failure = assertThrows(
                ElectricalWorld.SubscriptionOperationException.class,
                () -> world.subscribeObserver(deviceId, generation, 0)
            );

            assertFalse(failure.isOwnershipConsistencyFailure());
        }
    }

    @Test
    void isOpenTracksWorldLifetime() {
        try (ElectricalEngine engine = ElectricalEngine.create()) {
            ElectricalWorld world = engine.createWorld(30);

            assertTrue(world.isOpen());

            world.close();

            assertFalse(world.isOpen());
            assertDoesNotThrow(world::close);
        }
    }
}