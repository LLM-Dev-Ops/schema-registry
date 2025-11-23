"use strict";
/**
 * Type definitions for the LLM Schema Registry TypeScript SDK
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.CompatibilityMode = exports.SchemaFormat = void 0;
/** Supported schema formats */
var SchemaFormat;
(function (SchemaFormat) {
    SchemaFormat["JSON_SCHEMA"] = "json_schema";
    SchemaFormat["AVRO"] = "avro";
    SchemaFormat["PROTOBUF"] = "protobuf";
})(SchemaFormat || (exports.SchemaFormat = SchemaFormat = {}));
/** Schema compatibility modes */
var CompatibilityMode;
(function (CompatibilityMode) {
    CompatibilityMode["BACKWARD"] = "backward";
    CompatibilityMode["FORWARD"] = "forward";
    CompatibilityMode["FULL"] = "full";
    CompatibilityMode["BACKWARD_TRANSITIVE"] = "backward_transitive";
    CompatibilityMode["FORWARD_TRANSITIVE"] = "forward_transitive";
    CompatibilityMode["FULL_TRANSITIVE"] = "full_transitive";
    CompatibilityMode["NONE"] = "none";
})(CompatibilityMode || (exports.CompatibilityMode = CompatibilityMode = {}));
//# sourceMappingURL=types.js.map