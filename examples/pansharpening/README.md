# Configuration for: Pansharpening

## Breaking Down the Configuration
- Specify the input directory or series of directories:
    - Use the `input` value to parse through imagery in parallel. 
        - In the associated `config.json`, multiple directories are listed. Note that when listing multiple directories, they are listed within `[ ]`, signaling to Xylem these specific directories should be concurrently processed.
        ```json
        "input": [
            "/data/test-cases/test-case-GE01-medium-low-mid",
            "/data/test-cases/test-case-WV02-medium-mid-high",
            "/data/test-cases/test-case-wv03-large-high-mid",
            "/data/test-cases/test-case-wv03-large-mid-low"
        ]
        ```
        - A single directory may also be specified. Xylem will parse through this main directory and concurrently process all available subdirectories.
        ```json
        "input": 
            "/data/test-cases"
        ```
- Define the module:
    - Within the module variable, specify the `name` and `uri` to the module. The `uri` specifically informs Xylem from where to install the module environment. For example, if a user has Xylem installed and has cloned each of the modules in their `dev` directory within a Docker container, this path may look something like:
    ```json
        "uri": "file:///root/dev/module-p"
    ```
- Define the module variables:
    - Variables are unique to each module (detailed below).
- Simply follow the `template`:
    - This section is built from the module's `Makefile`. Here, users can specify arguments and associated variables. Below is an example template for Module P:
    ```json
        "template": {
                "command": "python",
                "environment": {
                    "name": "module-p",
                    "manager": "conda"
                },
                "arguments": [
                    "-m",
                    "lib",
                    "--source_directory",
                    "{{ INPUT }}",
                    "--output_directory",
                    "{{ OUTPUT_DIRECTORY }}",
                    "--method",
                    "{{ METHOD }}",
                    "--module_list",
                    "{{ MODULE_LIST }}"
                ]
            }
    ```

## Module Variables
For more thorough discussions on variables and overall structure, along with a link to technical documentation, please browse the README:
- [Module P](https://code-int.ornl.gov/gshs/common/imagery-processing/module-p/-/blob/main/README.md)

### Module P: ***P***ansharpening
- `source_directory`: the directory containing the raw, Level 1B Maxar imagery (or the output of Module A, if the images have been previously processed through Module A). When Module P is run as a standalone module, as with this `config.json`, this argument will be `INPUT` in the template.
- `output_directory`: the directory in which to store the output of Module P processing.
- `method`: specification for users to define a pansharpening method. Currently, only `nn_diffuse` is supported.
- `module_list`: specification for users to identify all modules used in the given configuration. The presence, or absence, of the string `MODA` in this argument determines how the input directories are processed. If the imagery has already been processed with Module A, include Module A on the list, even if you are only processing with Module P following the example `config.json`. Options for this argument are: 'MODP', 'MODP, MODO', 'MODA, MODP', 'MODA, MODP, MODO'.

## Running the Module
- To run the standalone module independently of any workflow:
    ```bash
    make run
    ```
- To run the module with Xylem, at maximum verbosity, using the `config.json`:
    ```bash
    xylem run -vvvv
    ```