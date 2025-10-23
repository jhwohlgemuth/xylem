# Configuration for: Atmospheric Correction

## Breaking Down the Configuration
- Specify the input directory or series of directories:
    - Use the `input` value to parse through imagery in parallel. 
        - In the associated `config.json`, there is a single directory listed. Xylem will parse through this main directory and concurrently process all available subdirectories.
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
- Define the module:
    - Within the module variable, specify the `name` and `uri` to the module. The `uri` specifically informs Xylem from where to install the module environment. For example, if a user has Xylem installed and has cloned each of the modules in their `dev` directory within a Docker container, this path may look something like:
    ```json
        "uri": "file:///root/dev/module-a"
    ```
- Define the module variables:
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

### Module A: ***A***tmopsheric Correction
- `input_directory`: the directory containing the raw, Level 1B Maxar imagery. Module A currently expects a Maxar data directory structure. Because Module A is either run as a standalone module or always the first in a multi-module sequence, this argument will always be `INPUT` in the template. 
- `output_directory`: the directory in which to store the output of Module A processing.
- `method`: specification for users to define if they want the output to contain top-of-atmosphere reflectance (`toa_reflectance`) or bottom-of-atmosphere reflectance (`boa_reflectance`). For the best representation of true surface reflectance (e.g., the removal of the blue effects of the atmosphere), users should select the latter.
- `profile`: specification for users to define the aersol profile selected in the Py6S model. Module A is currently optimized for `urban` applications, so consider using `maritime` sparingly. 
    - Future work for Module A will incorporate the maritime-optimized atmospheric correction efforts led by Matt McCarthy.

## Running the Module
- To run the standalone module independently of any workflow:
    ```bash
    make run
    ```
- To run the module with Xylem, at maximum verbosity, using the `config.json`:
    ```bash
    xylem run -vvvv
    ```