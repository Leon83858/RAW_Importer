using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace RawImporterCS;

public class MainWindowViewModel : INotifyPropertyChanged
{
    private bool _createSubfoldersPerFileType;

    public bool CreateSubfoldersPerFileType
    {
        get => _createSubfoldersPerFileType;
        set => SetProperty(ref _createSubfoldersPerFileType, value);
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private void SetProperty<T>(ref T field, T value, [CallerMemberName] string? propertyName = null)
    {
        if (Equals(field, value))
        {
            return;
        }

        field = value;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}
