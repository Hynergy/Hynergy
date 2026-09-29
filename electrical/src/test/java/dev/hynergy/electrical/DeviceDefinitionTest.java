package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class DeviceDefinitionTest {

    @Test
    void zeroDefinitionIdIsRejected() {
        assertThrows(IllegalArgumentException.class, () -> new DeviceDefinition(0));
    }

    @Test
    void definitionIdUsesFullUnsignedRange() {
        DeviceDefinition definition = new DeviceDefinition(-1);

        assertEquals(-1, definition.getId());
        assertEquals(0xffff_ffffL, Integer.toUnsignedLong(definition.getId()));
    }
}