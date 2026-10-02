package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class WireIdTest {

    @Test
    void identityRequiresPositiveValueAndNonZeroGeneration() {
        assertThrows(IllegalArgumentException.class, () -> new WireId(0, 1));
        assertThrows(IllegalArgumentException.class, () -> new WireId(-1, 1));
        assertThrows(IllegalArgumentException.class, () -> new WireId(1, 0));
    }

    @Test
    void packedRoundTripPreservesSignedGeneration() {
        WireId wireId = new WireId(7, -1);

        assertEquals(wireId, WireId.fromPacked(wireId.packed()));
    }

    @Test
    void invalidPackedIdentityIsRejected() {
        assertThrows(IllegalArgumentException.class, () -> WireId.fromPacked(0L));
    }
}
