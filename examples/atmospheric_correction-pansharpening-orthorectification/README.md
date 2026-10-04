# Configuration for: Atmospheric Correction, Pansharpening, and Orthorectification

## Breaking Down the Configuration
- Specify the input directory or series of directories:
    - Use the `input` value to parse through imagery in parallel. 
        - In the associated `.xylem.json`, there is a single directory listed. Xylem will parse through this main directory and concurrently process all available subdirectories.
        ```json
        "input": 
            "/data/test-cases"
        ```
        - Multiple directories may also be specified. Note that when listing multiple directories, they are listed within `[ ]`, signaling to Xylem these specific directories should be concurrently processed.
        ```json
        "input": [
            "/data/test-cases/test-case-GE01-medium-low-mid",
            "/data/test-cases/test-case-WV02-medium-mid-high",
            "/data/test-cases/test-case-wv03-large-high-mid",
            "/data/test-cases/test-case-wv03-large-mid-low"
        ]
        ```
- Define each module:
    - Within the module variable, specify the `name` and `uri` to the module. The `uri` specifically informs Xylem from where to install the module environment. For example, if a user has Xylem installed and has cloned each of the modules in their `dev` directory within a Docker container, this path may look something like:
    ```json
        "uri": "file:///root/dev/module-a"
    ```
- Define each module's variables:
    - Variables are unique to each module (detailed below).
- Simply follow the `template`:
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

## Module Variables
For more thorough discussions on variables and overall structure, along with a link to technical documentation, please browse the README:
- [Module A](https://code-int.ornl.gov/gshs/common/imagery-processing/module-a/-/blob/main/README.md)
- [Module P](https://code-int.ornl.gov/gshs/common/imagery-processing/module-p/-/blob/main/README.md)
- [Module O](https://code-int.ornl.gov/gshs/common/imagery-processing/module-o/-/blob/main/README.md)

### Module A: ***A***tmopsheric Correction
- `input_directory`: the directory containing the raw, Level 1B Maxar imagery. Module A currently expects a Maxar data directory structure. This argument will always be `INPUT` in the template. 
- `output_directory`: the directory in which to store the output of Module A processing.
- `method`: specification for users to define if they want the output to contain top-of-atmosphere reflectance (`toa_reflectance`) or bottom-of-atmosphere reflectance (`boa_reflectance`). For the best representation of true surface reflectance (e.g., the removal of the blue effects of the atmosphere), users should select the latter.
- `profile`: specification for users to define the aersol profile selected in the Py6S model. Module A is currently optimized for `urban` applications, so consider using `maritime` sparingly. 
    - Future work for Module A will incorporate the maritime-optimized atmospheric correction efforts led by Matt McCarthy.

### Module P: ***P***ansharpening
- `source_directory`: the directory containing the output of Module A. This argument should match the `output_directory` of Module A.
- `output_directory`: the directory in which to store the output of Module P processing. This argument should match both the `source_directory` for Module P and the `output_directory` of Module A.
- `method`: specification for users to define a pansharpening method. Currently, only `nn_diffuse` is supported.
- `module_list`: specification for users to identify all modules used in the given configuration. The presence, or absence, of the string `MODA` in this argument determines how the input directories are processed. In this example `.xylem.json`, users would specify Modules A, P, and O. Options for this argument are: 'MODP', 'MODP, MODO', 'MODA, MODP', 'MODA, MODP, MODO'.

### Module O: ***O***rthorectification
- 'source_directory': the directory containing the output of Module P. This argument should match the `output_directory` of Module P.
- 'output_directory': the directory in which to store the output of Module O processing. This argument should match both the `source_directory` for Module O and the `output_directory` of Module P.

## Running the Module
- To run the standalone module independently of any workflow:
    ```bash
    make run
    ```
- To run the module with Xylem, at maximum verbosity, using the automatically discovered `.xylem.json`:
    ```bash
    xylem run -vvvv
    ```

## General Notes for a Multi-Module Configuration
- The general structure of the configuration remains the same:
    - Specify the input directory or series of directories:
        - There will still be the initial `input` value to include outside of the module definitions. This is generally going to be the directory (or specific subdirectories) of raw, level 1B imagery.
    - Define the modules:
        - Just as with a standalone configuration, users will need to identify all of the modules being used for processing, in the correct order and correct paths.
    - Define each module's variables:
        - All variables are defined as above. However, the only caveat with the multi-module implementation is aligning the input/source and output directories between modules.
            - **The output of a given module will always be the same path as the input to the following module.**
            - **All non-primary modules will have the same path for their source and output directories.**
    - Follow the template:
        - Just as with a standalone configuration, users can specify module arguments within each module's template.
- The multi-module configuration will provide outputs at each stage of processing, which are used as input to subsequent stages. This design is meant to support easier troubleshooting between modules, if needed.
- As imagery is processed, output files will be appended with the name of the processing module. Therefore, imagery processed with Modules A, P, and O will have filenames ending in `MODA_MODP_MODO`. 
    - Module P also updates the middle of the filenames from `P1BS` for panchromatic imagery and `M1BS` for multispectral imagery to `S3XS` which is a Maxar naming standard and a naming convention upheld throughout the use of Legacy PIPE.
- For multi-module processing, combinations can **only** be made in these two orders:
    > Module A -> Module P -> Module O
    
    > Module P -> Module O
    - Atmospheric correction is **always** the first processing step if it is included, and imagery **must** be pansharpened before it is orthorectified.

## Resuming

Run with `xylem run --resume` to skip a stage whose declared output pattern and
completion marker are both current. Resume operates on each top-level input
directory; it does not resume individual image pairs managed internally by a
module.
The top-level `output` value supplies the shared `OUTPUT_DIRECTORY`; use
`--artifact-output` to override that artifact root for one run.

The configuration's scheduling contract makes each immediate child of
`/data/test-cases` one ordered top-level input. A write-free dry run with two
representative test-case directories verified that both inputs retain all three
modules in declaration order.
`options.batch: 2` therefore limits active test-case directories, not image
pairs or threads created inside Modules A, P, or O.
