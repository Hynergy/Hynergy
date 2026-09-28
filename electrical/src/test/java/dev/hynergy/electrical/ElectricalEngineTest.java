package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

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
}