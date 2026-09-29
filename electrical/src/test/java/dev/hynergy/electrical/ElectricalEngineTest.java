package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class ElectricalEngineTest {

    @Test
    void onlyOneEngineMayBeLiveAtATime() {
        try (ElectricalEngine first = ElectricalEngine.create()) {
            assertThrows(IllegalStateException.class, ElectricalEngine::create);
            assertThrows(IllegalStateException.class, ElectricalEngine::create);
        }
    }

    @Test
    void newEngineMayBeCreatedAfterPreviousEngineCloses() {
        ElectricalEngine first = ElectricalEngine.create();

        first.close();

        try (ElectricalEngine second = ElectricalEngine.create()) {
            assertNotNull(second);
        }
    }

    @Test
    void repeatedCloseDoesNotBreakEngineOwnership() {
        ElectricalEngine first = ElectricalEngine.create();

        first.close();
        first.close();

        try (ElectricalEngine second = ElectricalEngine.create()) {
            assertNotNull(second);
        }
    }

    @Test
    void registerDefinitionReturnsAssignedDefinition() {
        try (ElectricalEngine engine = ElectricalEngine.create();
            DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder()) {
            DeviceDefinition definition = engine.registerDefinition(builder);

            assertNotNull(definition);
            assertNotEquals(0, definition.getId());
        }
    }

    @Test
    void registrationsReceiveDistinctDefinitionIds() {
        try (ElectricalEngine engine = ElectricalEngine.create();
            DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder()) {
            DeviceDefinition first = engine.registerDefinition(builder);
            DeviceDefinition second = engine.registerDefinition(builder);

            assertNotEquals(first.getId(), second.getId());
        }
    }

    @Test
    void successfulRegistrationDoesNotResetBuilder() {
        try (ElectricalEngine engine = ElectricalEngine.create();
            DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder()) {
            int firstTerminal = builder.addTerminal();
            int secondTerminal = builder.addTerminal();

            builder.beginElement(new DeviceDefinition(1))
                .elementTerminal(firstTerminal)
                .elementTerminal(secondTerminal)
                .elementLiteral(1_000.0)
                .endElement();

            long byteSize = builder.byteSize();
            long commandCount = builder.commandCount();

            DeviceDefinition definition = engine.registerDefinition(builder);

            assertNotEquals(0, definition.getId());
            assertEquals(byteSize, builder.byteSize());
            assertEquals(commandCount, builder.commandCount());
            assertEquals(3, builder.commandCount());
        }
    }

    @Test
    void rejectedDefinitionDoesNotPoisonRegistration() {
        try (ElectricalEngine engine = ElectricalEngine.create();
            DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder()) {
            builder.addTerminal();
            builder.addNode();

            assertThrows(IllegalArgumentException.class, () -> engine.registerDefinition(builder));

            builder.reset();

            DeviceDefinition definition = assertDoesNotThrow(() -> engine.registerDefinition(builder));

            assertNotEquals(0, definition.getId());
        }
    }
}