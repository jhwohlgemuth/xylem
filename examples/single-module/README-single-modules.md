# Examples of Single Module Configurations
> For geospatial data processing, Modules A and P are available to run as standalone modules. Module O currently requires Module P processing first, and therefore cannot be standalone. See the multi-module configuration options to incorporate Module O processing. 

## Understanding the Entire Config
Single module configurations are relatively straightforward, as there are no variables to align between multiple modules. Instead, users simply need to modify four main parts of the config based on their specific processing needs.

- Specify the input directory or series of directories:
    - All standalone modules will use the `input` value to parse through imagery in parallel. 
        - In `module-a-config.json`, there is a single directory listed. PIPE will parse through this main directory and concurrently process all available subdirectories.
        ```json
        "input": 
            "/path/to/input/directory"
        ```
        - In `module-p-config.json`, there are multiple directories listed. Note that when listing multiple directories, they are listed within `[ ]`, signaling to PIPE these specific directories should be concurrently processed.
        ```json
        "input": [
            "/path/to/input/order1",
            "/path/to/input/order2",
            "/path/to/input/order3",
            "/path/to/input/order4"
        ]
        ```
    - Modules A and P can both use either of these `input` configurations; they are not limited to strictly following the examples shown here. These input variables are only different between example `config.json` files to showcase the variation in `input` options.
- Define the module:
    - Within the module variable, specify the `name` and `uri` to the module. The `uri` specifically informs PIPE from where to install the module environment. For example, if a user has PIPE installed and has cloned each of the modules in their `dev` directory within a Docker container, this path may look something like:
    ```json
        "uri": "file:///root/dev/module-a"
    ```
- Define the module variables:
    - Variables are unique to each module (details below).
    - For standalone modules specifically, pay close attention to the `input`: users do **not** need to specify `input` as a variable within the module. Rather, the `input` is noted in the `template`.
- Speaking of the `template`:
    - This section is built from the module's `Makefile`. Here, users can specify arguments and associated variables. Below is an example template for Module A:
    ```json
        "template": {
                "command": "python",
                "environment": {
                    "name": "module-a",
                    "manager": "conda"
                },
                "arguments": [
                    "-m",
                    "lib",
                    "--input_directory",
                    "{{ INPUT }}",
                    "--output_directory",
                    "{{ OUTPUT_DIRECTORY }}",
                    "--method",
                    "{{ METHOD }}",
                    "--profile",
                    "{{ PROFILE }}"
                ]
            }
    ```
    - **NOTE**: 
        - For standalone modules the `input_directory` for Module A and `source_directory` for Module P will **always** be `INPUT` in the template.

## Module Variables
While Module O is not an option as a standalone module, the variable definitions are included here as well for continuity. For more thorough discussions on variables and structure for individual modules, please browse the README for each:
- [Module A](https://code.ornl.gov/GSHS/common/pipe/module-a/-/blob/main/README.md)
- [Module P](https://code.ornl.gov/GSHS/common/pipe/module-p/-/blob/main/README.md)
- [Module O](https://code.ornl.gov/GSHS/common/pipe/module-o/-/blob/main/README.md)

### Module A: ***A***tmopsheric Correction
- `input_directory`: the directory containing the raw, Level 1B Maxar imagery. Module A currently expects a Maxar data directory structure. Because Module A is either run as a standalone module or always the first in a multi-module sequence, this argument will always be `INPUT` in the template. 
- `output_directory`: the directory in which to store the output of Module A processing.
- `method`: specification for users to define if they want the output to contain top-of-atmosphere reflectance (`toa_reflectance`) or bottom-of-atmosphere reflectance (`boa_reflectance`). For the best representation of true surface reflectance (e.g., the removal of the blue effects of the atmosphere), users should select the latter.
- `profile`: specification for users to define the aersol profile selected in the Py6S model. Module A is currently optimized for `urban` applications, so consider using `maritime` sparingly. 
    - Future work for Module A will incorporate the maritime-optimized atmospheric correction efforts led by Matt McCarthy.

### Module P: ***P***ansharpening
- `source_directory`: the directory containing the raw, Level 1B Maxar imagery (or the output of Module A for multi-module implementations). When Module P is run as a standalone module, this argument will be `INPUT` in the template. For multi-module configurations, this argument should match the `output_directory` of Module A.
- `output_directory`: the directory in which to store the output of Module P processing. For multi-module configurations, this argument should match both the `source_directory` for Module P and the `output_directory` of Module A.
- `method`: specification for users to define a pansharpening method. Currently, only `nn_diffuse` is supported.
- `module_list`: specification for users to identify all modules used in the given configuration. The presence, or absence, of the string `MODA` in this argument determines how the input directories are processed, as Module P can be used standalone (where input is a Maxar directory) or as a multi-module configuration (where input is the output of Module A). Options for this argument are: 'MODP', 'MODP, MODO', 'MODA, MODP', 'MODA, MODP, MODO'.

### Module O: ***O***rthorectification
- 'source_directory': the directory containing the output of Module P. This argument should match the `output_directory` of Module P, as Module O is only supported in mutli-module configurations.
- 'output_directory': the directory in which to store the output of Module O processing. This argument should match both the `source_directory` for Module O and the `output_directory` of Module P, as Module O is only supported in multi-module configurations.