struct ModelMatrix{
    mat4 model;
    mat4 normal;
};

struct MaterialParameters {
    vec4 diffuse;
    vec4 emissive;
    ivec4 textures;
    ivec4 textures_two;
};

layout(std140,set = 1, binding = 2) readonly buffer ModelBuffer{
    ModelMatrix models[];
} modelData;

layout(std140,set = 1, binding = 3) readonly buffer MaterialBuffer{
    MaterialParameters materials[];
} materialData;