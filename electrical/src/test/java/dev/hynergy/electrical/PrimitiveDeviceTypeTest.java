package dev.hynergy.electrical;

import dev.hynergy.electrical.primitives.controlled.VoltageControlledConductance;
import dev.hynergy.electrical.primitives.controlled.VoltageControlledSwitch;
import dev.hynergy.electrical.primitives.logic.*;
import dev.hynergy.electrical.primitives.passive.*;
import dev.hynergy.electrical.primitives.sources.CurrentSource;
import dev.hynergy.electrical.primitives.sources.VoltageControlledCurrentSource;
import dev.hynergy.electrical.primitives.sources.VoltageControlledVoltageSource;
import dev.hynergy.electrical.primitives.sources.VoltageSource;
import org.junit.jupiter.api.Test;

import java.util.Objects;

import static org.junit.jupiter.api.Assertions.assertEquals;

final class PrimitiveDeviceTypeTest {

    @Test
    void primitiveTypesMatchNativeDefinitionIds() {
        assertDefinitionId(1, Resistance.TYPE);
        assertDefinitionId(2, Conductance.TYPE);
        assertDefinitionId(3, VoltageSource.TYPE);
        assertDefinitionId(4, CurrentSource.TYPE);
        assertDefinitionId(5, VoltageControlledCurrentSource.TYPE);
        assertDefinitionId(6, VoltageControlledVoltageSource.TYPE);
        assertDefinitionId(7, Capacitor.TYPE);
        assertDefinitionId(8, Inductor.TYPE);
        assertDefinitionId(9, VoltageControlledSwitch.TYPE);
        assertDefinitionId(10, VoltageControlledConductance.TYPE);
        assertDefinitionId(11, TickDelay.TYPE);
        assertDefinitionId(12, Diode.TYPE);
        assertDefinitionId(13, Not.TYPE);
        assertDefinitionId(14, And.TYPE);
        assertDefinitionId(15, Nand.TYPE);
        assertDefinitionId(16, Or.TYPE);
        assertDefinitionId(17, Nor.TYPE);
        assertDefinitionId(18, SchmittBuffer.TYPE);
    }

    private static void assertDefinitionId(int expectedId, DeviceType<?> type) {
        DeviceDefinition definition =
            Objects.requireNonNull(type.currentDefinition(), "Primitive definition is missing");

        assertEquals(expectedId, definition.id());
    }
}
