package dev.hynergy.core.electricity.wire;

import com.hypixel.hytale.codec.Codec;
import com.hypixel.hytale.codec.KeyedCodec;
import com.hypixel.hytale.codec.builder.BuilderCodec;
import com.hypixel.hytale.codec.schema.SchemaContext;
import com.hypixel.hytale.codec.schema.config.Schema;
import com.hypixel.hytale.codec.validation.ValidationResults;
import com.hypixel.hytale.codec.validation.Validator;
import com.hypixel.hytale.codec.validation.Validators;
import com.hypixel.hytale.math.vector.Vector3iUtil;
import lombok.Getter;
import org.joml.Vector3i;

public final class WirePortConfig {

    private static final Validator<Vector3i> CARDINAL_NORMAL_VALIDATOR =
            new Validator<>() {
                @Override
                public void accept(Vector3i value, ValidationResults results) {
                    if (value == null) {
                        return;
                    }

                    int x = value.x;
                    int y = value.y;
                    int z = value.z;

                    boolean cardinal =
                            (x == 1 || x == -1) && y == 0 && z == 0
                                    || (y == 1 || y == -1) && x == 0 && z == 0
                                    || (z == 1 || z == -1) && x == 0 && y == 0;

                    if (!cardinal) {
                        results.fail(
                                "Wire port Normal must be one of the six cardinal unit vectors"
                        );
                    }
                }

                @Override
                public void updateSchema(SchemaContext context, Schema target) {
                }
            };

    public static final BuilderCodec<WirePortConfig> CODEC =
            BuilderCodec.builder(WirePortConfig.class, WirePortConfig::new)
                        .append(
                                new KeyedCodec<>("Id", Codec.INTEGER, true),
                                (port, id) -> port.id = id,
                                port -> port.id
                        )
                        .addValidator(Validators.greaterThanOrEqual(0))
                        .documentation(
                                "Stable local identifier of this port within the wire block."
                        )
                        .add()
                        .append(
                                new KeyedCodec<>("Anchor", Vector3iUtil.CODEC),
                                (port, anchor) -> port.anchor = anchor,
                                port -> port.anchor
                        )
                        .documentation(
                                "Optional block-local anchor of the port. "
                                        + "Omitting it means the owning block origin."
                        )
                        .add()
                        .append(
                                new KeyedCodec<>("Normal", Vector3iUtil.CODEC, true),
                                (port, normal) -> port.normal = normal,
                                port -> port.normal
                        )
                        .addValidator(CARDINAL_NORMAL_VALIDATOR)
                        .documentation(
                                "Outward-facing local cardinal normal of the electrical connection."
                        )
                        .add()
                        .build();

    @Getter private int id;
    @Getter private Vector3i anchor;
    @Getter private Vector3i normal;

    private WirePortConfig() {
    }
}